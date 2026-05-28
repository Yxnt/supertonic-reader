// ============================================================================
// TTS Helper Module - Adapted from supertone-inc/supertonic
// ============================================================================

use ndarray::{Array, Array3};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use anyhow::{Result, bail};
use unicode_normalization::UnicodeNormalization;
use hound::{WavWriter, WavSpec, SampleFormat};
use rand_distr::{Distribution, Normal};
use regex::Regex;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

#[derive(Clone, Copy, Debug)]
pub enum SessionKind {
    DurationPredictor,
    TextEncoder,
    VectorEstimator,
    Vocoder,
}

fn recommended_intra_threads() -> usize {
    if let Ok(v) = std::env::var("TTS_INTRA_THREADS") {
        if let Ok(n) = v.parse::<usize>() {
            return n.max(1);
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(2, 4)
}

fn want_dml(kind: SessionKind) -> bool {
    if !cfg!(target_os = "windows") { return false; }
    if std::env::var("TTS_DISABLE_DML").ok().as_deref() == Some("1") { return false; }
    matches!(kind, SessionKind::VectorEstimator | SessionKind::Vocoder)
}

pub fn build_session<P: AsRef<Path>>(onnx_path: P, kind: SessionKind) -> Result<Session> {
    let intra = recommended_intra_threads();
    let map = |e: ort::Error<ort::session::builder::SessionBuilder>| anyhow::anyhow!("ort builder: {e}");

    let make_cpu = || -> Result<ort::session::builder::SessionBuilder> {
        let mut b = Session::builder()?;
        b = b
            .with_optimization_level(GraphOptimizationLevel::Level2).map_err(map)?
            .with_intra_threads(intra).map_err(map)?
            .with_inter_threads(1).map_err(map)?
            .with_parallel_execution(false).map_err(map)?
            .with_memory_pattern(true).map_err(map)?;
        Ok(b)
    };

    // DirectML EP constraints (from onnxruntime docs):
    //  - graph_optimization_level must be <= Level1 (Basic), higher will be rejected
    //  - memory pattern must be disabled
    let make_dml = || -> Result<ort::session::builder::SessionBuilder> {
        use ort::ep::DirectML;
        let mut b = Session::builder()?;
        b = b
            .with_optimization_level(GraphOptimizationLevel::Level1).map_err(map)?
            .with_intra_threads(intra).map_err(map)?
            .with_inter_threads(1).map_err(map)?
            .with_parallel_execution(false).map_err(map)?
            .with_memory_pattern(false).map_err(map)?;
        b = b.with_execution_providers([DirectML::default().build()])
            .map_err(|e| anyhow::anyhow!("dml register: {e}"))?;
        Ok(b)
    };

    let mut builder = if want_dml(kind) {
        match make_dml() {
            Ok(b) => {
                eprintln!("[tts] {:?}: DirectML EP registered", kind);
                b
            }
            Err(e) => {
                eprintln!("[tts] {:?}: DirectML setup failed, using CPU: {}", kind, e);
                make_cpu()?
            }
        }
    } else {
        make_cpu()?
    };

    let session = builder.commit_from_file(onnx_path)?;
    Ok(session)
}

pub const AVAILABLE_LANGS: &[&str] = &[
    "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi", "hr", "hu", "id", "it", "lt", "lv", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "tr", "uk", "vi"
];

pub fn is_valid_lang(lang: &str) -> bool {
    AVAILABLE_LANGS.contains(&lang)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ae: AEConfig,
    pub ttl: TTLConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AEConfig {
    pub sample_rate: i32,
    pub base_chunk_size: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTLConfig {
    pub chunk_compress_factor: i32,
    pub latent_dim: i32,
}

pub fn load_cfgs<P: AsRef<Path>>(onnx_dir: P) -> Result<Config> {
    let cfg_path = onnx_dir.as_ref().join("tts.json");
    let file = File::open(cfg_path)?;
    let reader = BufReader::new(file);
    let cfgs: Config = serde_json::from_reader(reader)?;
    Ok(cfgs)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceStyleData {
    pub style_ttl: StyleComponent,
    pub style_dp: StyleComponent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleComponent {
    pub data: Vec<Vec<Vec<f32>>>,
    pub dims: Vec<i64>,
    #[serde(rename = "type")]
    pub dtype: String,
}

pub struct UnicodeProcessor {
    indexer: Vec<i64>,
}

impl UnicodeProcessor {
    pub fn new<P: AsRef<Path>>(unicode_indexer_json_path: P) -> Result<Self> {
        let file = File::open(unicode_indexer_json_path)?;
        let reader = BufReader::new(file);
        let indexer: Vec<i64> = serde_json::from_reader(reader)?;
        Ok(UnicodeProcessor { indexer })
    }

    pub fn call(&self, text_list: &[String], lang_list: &[String]) -> Result<(Vec<Vec<i64>>, Array3<f32>)> {
        let mut processed_texts: Vec<String> = Vec::new();
        for (text, lang) in text_list.iter().zip(lang_list.iter()) {
            processed_texts.push(preprocess_text(text, lang)?);
        }

        let text_ids_lengths: Vec<usize> = processed_texts.iter().map(|t| t.chars().count()).collect();
        let max_len = *text_ids_lengths.iter().max().unwrap_or(&0);

        let mut text_ids = Vec::new();
        for text in &processed_texts {
            let mut row = vec![0i64; max_len];
            let unicode_vals = text_to_unicode_values(text);
            for (j, &val) in unicode_vals.iter().enumerate() {
                if val < self.indexer.len() {
                    row[j] = self.indexer[val];
                } else {
                    row[j] = -1;
                }
            }
            text_ids.push(row);
        }

        let text_mask = get_text_mask(&text_ids_lengths);
        Ok((text_ids, text_mask))
    }
}

pub fn preprocess_text(text: &str, lang: &str) -> Result<String> {
    let mut text: String = text.nfkd().collect();

    let emoji_pattern = Regex::new(r"[\x{1F600}-\x{1F64F}\x{1F300}-\x{1F5FF}\x{1F680}-\x{1F6FF}\x{1F700}-\x{1F77F}\x{1F780}-\x{1F7FF}\x{1F800}-\x{1F8FF}\x{1F900}-\x{1F9FF}\x{1FA00}-\x{1FA6F}\x{1FA70}-\x{1FAFF}\x{2600}-\x{26FF}\x{2700}-\x{27BF}\x{1F1E6}-\x{1F1FF}]+").unwrap();
    text = emoji_pattern.replace_all(&text, "").to_string();

    let replacements = [
        ("–", "-"), ("‑", "-"), ("—", "-"), ("_", " "),
        ("\u{201C}", "\""), ("\u{201D}", "\""), ("\u{2018}", "'"), ("\u{2019}", "'"),
        ("´", "'"), ("`", "'"), ("[", " "), ("]", " "),
        ("|", " "), ("/", " "), ("#", " "), ("→", " "), ("←", " "),
    ];
    for (from, to) in &replacements {
        text = text.replace(from, to);
    }

    let special_symbols = ["♥", "☆", "♡", "©", "\""];
    for symbol in &special_symbols {
        text = text.replace(symbol, "");
    }

    let expr_replacements = [("@", " at "), ("e.g.,", "for example, "), ("i.e.,", "that is, ")];
    for (from, to) in &expr_replacements {
        text = text.replace(from, to);
    }

    text = Regex::new(r" ,").unwrap().replace_all(&text, ",").to_string();
    text = Regex::new(r" \.").unwrap().replace_all(&text, ".").to_string();
    text = Regex::new(r" !").unwrap().replace_all(&text, "!").to_string();
    text = Regex::new(r" \?").unwrap().replace_all(&text, "?").to_string();
    text = Regex::new(r" ;").unwrap().replace_all(&text, ";").to_string();
    text = Regex::new(r" :").unwrap().replace_all(&text, ":").to_string();
    text = Regex::new(r" '").unwrap().replace_all(&text, "'").to_string();

    while text.contains("\"\"") { text = text.replace("\"\"", "\""); }
    while text.contains("''") { text = text.replace("''", "'"); }
    while text.contains("``") { text = text.replace("``", "`"); }

    text = Regex::new(r"\s+").unwrap().replace_all(&text, " ").to_string();
    text = text.trim().to_string();

    if !text.is_empty() {
        let ends_with_punct = Regex::new(r#"[.!?;:,'"\u{201C}\u{201D}\u{2018}\u{2019})\]}…。」』】〉》›»]$"#).unwrap();
        if !ends_with_punct.is_match(&text) {
            text.push('.');
        }
    }

    if !is_valid_lang(lang) {
        bail!("Invalid language: {}. Available: {:?}", lang, AVAILABLE_LANGS);
    }

    text = format!("<{}>{}</{}>", lang, text, lang);
    Ok(text)
}

pub fn text_to_unicode_values(text: &str) -> Vec<usize> {
    text.chars().map(|c| c as usize).collect()
}

pub fn length_to_mask(lengths: &[usize], max_len: Option<usize>) -> Array3<f32> {
    let bsz = lengths.len();
    let max_len = max_len.unwrap_or_else(|| *lengths.iter().max().unwrap_or(&0));
    let mut mask = Array3::<f32>::zeros((bsz, 1, max_len));
    for (i, &len) in lengths.iter().enumerate() {
        for j in 0..len.min(max_len) {
            mask[[i, 0, j]] = 1.0;
        }
    }
    mask
}

pub fn get_text_mask(text_ids_lengths: &[usize]) -> Array3<f32> {
    let max_len = *text_ids_lengths.iter().max().unwrap_or(&0);
    length_to_mask(text_ids_lengths, Some(max_len))
}

pub fn sample_noisy_latent(
    duration: &[f32],
    sample_rate: i32,
    base_chunk_size: i32,
    chunk_compress: i32,
    latent_dim: i32,
) -> (Array3<f32>, Array3<f32>) {
    let bsz = duration.len();
    let max_dur = duration.iter().fold(0.0f32, |a, &b| a.max(b));
    let wav_len_max = (max_dur * sample_rate as f32) as usize;
    let wav_lengths: Vec<usize> = duration.iter().map(|&d| (d * sample_rate as f32) as usize).collect();

    let chunk_size = (base_chunk_size * chunk_compress) as usize;
    let latent_len = (wav_len_max + chunk_size - 1) / chunk_size;
    let latent_dim_val = (latent_dim * chunk_compress) as usize;

    let total = bsz * latent_dim_val * latent_len;
    let normal = Normal::new(0.0, 1.0).unwrap();
    let mut rng = rand::thread_rng();
    let mut buf: Vec<f32> = (0..total).map(|_| normal.sample(&mut rng)).collect();

    let latent_lengths: Vec<usize> = wav_lengths.iter().map(|&len| (len + chunk_size - 1) / chunk_size).collect();
    let latent_mask = length_to_mask(&latent_lengths, Some(latent_len));

    if latent_len > 0 {
        for b in 0..bsz {
            let active = latent_lengths[b].min(latent_len);
            for d in 0..latent_dim_val {
                let base = (b * latent_dim_val + d) * latent_len;
                for t in active..latent_len {
                    buf[base + t] = 0.0;
                }
            }
        }
    }

    let noisy_latent = Array3::from_shape_vec((bsz, latent_dim_val, latent_len), buf).unwrap();
    (noisy_latent, latent_mask)
}

pub fn write_wav_file<P: AsRef<Path>>(
    filename: P,
    audio_data: &[f32],
    sample_rate: i32,
) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(filename, spec)?;
    for &sample in audio_data {
        let clamped = sample.max(-1.0).min(1.0);
        let val = (clamped * 32767.0) as i16;
        writer.write_sample(val)?;
    }
    writer.finalize()?;
    Ok(())
}

const MAX_CHUNK_LENGTH: usize = 300;
const ABBREVIATIONS: &[&str] = &[
    "Dr.", "Mr.", "Mrs.", "Ms.", "Prof.", "Sr.", "Jr.",
    "St.", "Ave.", "Rd.", "Blvd.", "Dept.", "Inc.", "Ltd.",
    "Co.", "Corp.", "etc.", "vs.", "i.e.", "e.g.", "Ph.D.",
];

pub fn chunk_text(text: &str, max_len: Option<usize>) -> Vec<String> {
    let max_len = max_len.unwrap_or(MAX_CHUNK_LENGTH);
    let text = text.trim();
    if text.is_empty() { return vec![String::new()]; }

    let para_re = Regex::new(r"\n\s*\n").unwrap();
    let paragraphs: Vec<&str> = para_re.split(text).collect();
    let mut chunks = Vec::new();

    for para in paragraphs {
        let para = para.trim();
        if para.is_empty() { continue; }
        if para.len() <= max_len { chunks.push(para.to_string()); continue; }

        let sentences = split_sentences(para);
        let mut current = String::new();
        let mut current_len = 0;

        for sentence in sentences {
            let sentence = sentence.trim();
            if sentence.is_empty() { continue; }
            let sentence_len = sentence.len();

            if sentence_len > max_len {
                if !current.is_empty() { chunks.push(current.trim().to_string()); current.clear(); current_len = 0; }
                let parts: Vec<&str> = sentence.split(',').collect();
                for part in parts {
                    let part = part.trim();
                    if part.is_empty() { continue; }
                    let part_len = part.len();
                    if part_len > max_len {
                        let words: Vec<&str> = part.split_whitespace().collect();
                        let mut word_chunk = String::new();
                        let mut word_chunk_len = 0;
                        for word in words {
                            let word_len = word.len();
                            if word_chunk_len + word_len + 1 > max_len && !word_chunk.is_empty() {
                                chunks.push(word_chunk.trim().to_string());
                                word_chunk.clear(); word_chunk_len = 0;
                            }
                            if !word_chunk.is_empty() { word_chunk.push(' '); word_chunk_len += 1; }
                            word_chunk.push_str(word); word_chunk_len += word_len;
                        }
                        if !word_chunk.is_empty() { chunks.push(word_chunk.trim().to_string()); }
                    } else {
                        if current_len + part_len + 1 > max_len && !current.is_empty() {
                            chunks.push(current.trim().to_string()); current.clear(); current_len = 0;
                        }
                        if !current.is_empty() { current.push_str(", "); current_len += 2; }
                        current.push_str(part); current_len += part_len;
                    }
                }
                continue;
            }

            if current_len + sentence_len + 1 > max_len && !current.is_empty() {
                chunks.push(current.trim().to_string()); current.clear(); current_len = 0;
            }
            if !current.is_empty() { current.push(' '); current_len += 1; }
            current.push_str(sentence); current_len += sentence_len;
        }
        if !current.is_empty() { chunks.push(current.trim().to_string()); }
    }

    if chunks.is_empty() { vec![String::new()] } else { chunks }
}

fn split_sentences(text: &str) -> Vec<String> {
    let re = Regex::new(r"([.!?])\s+").unwrap();
    let matches: Vec<_> = re.find_iter(text).collect();
    if matches.is_empty() { return vec![text.to_string()]; }

    let mut sentences = Vec::new();
    let mut last_end = 0;

    for m in matches {
        let before_punc = &text[last_end..m.start()];
        let mut is_abbrev = false;
        for abbrev in ABBREVIATIONS {
            let combined = format!("{}{}", before_punc.trim(), &text[m.start()..m.start()+1]);
            if combined.ends_with(abbrev) { is_abbrev = true; break; }
        }
        if !is_abbrev {
            sentences.push(text[last_end..m.end()].to_string());
            last_end = m.end();
        }
    }
    if last_end < text.len() { sentences.push(text[last_end..].to_string()); }
    if sentences.is_empty() { vec![text.to_string()] } else { sentences }
}

pub fn sanitize_filename(text: &str, max_len: usize) -> String {
    text.chars().take(max_len).map(|c| if c.is_alphanumeric() { c } else { '_' }).collect()
}

pub struct Style {
    pub ttl: Array3<f32>,
    pub dp: Array3<f32>,
}

pub struct TextToSpeech {
    cfgs: Config,
    text_processor: UnicodeProcessor,
    dp_ort: Session,
    text_enc_ort: Session,
    vector_est_ort: Session,
    vocoder_ort: Session,
    pub sample_rate: i32,
}

impl TextToSpeech {
    pub fn new(
        cfgs: Config,
        text_processor: UnicodeProcessor,
        dp_ort: Session,
        text_enc_ort: Session,
        vector_est_ort: Session,
        vocoder_ort: Session,
    ) -> Self {
        let sample_rate = cfgs.ae.sample_rate;
        TextToSpeech { cfgs, text_processor, dp_ort, text_enc_ort, vector_est_ort, vocoder_ort, sample_rate }
    }

    fn _infer(
        &mut self,
        text_list: &[String],
        lang_list: &[String],
        style: &Style,
        total_step: usize,
        speed: f32,
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        use ort::value::TensorRef;

        let bsz = text_list.len();
        let (text_ids, text_mask) = self.text_processor.call(text_list, lang_list)?;

        let text_ids_array = {
            let text_ids_shape = (bsz, text_ids[0].len());
            let mut flat = Vec::with_capacity(text_ids_shape.0 * text_ids_shape.1);
            for row in &text_ids { flat.extend_from_slice(row); }
            Array::from_shape_vec(text_ids_shape, flat)?
        };

        let dp_outputs = self.dp_ort.run(ort::inputs!{
            "text_ids" => TensorRef::from_array_view(text_ids_array.view())?,
            "style_dp" => TensorRef::from_array_view(style.dp.view())?,
            "text_mask" => TensorRef::from_array_view(text_mask.view())?,
        })?;

        let (_, duration_data) = dp_outputs["duration"].try_extract_tensor::<f32>()?;
        let mut duration: Vec<f32> = duration_data.to_vec();
        for dur in duration.iter_mut() { *dur /= speed; }
        drop(dp_outputs);

        let text_enc_outputs = self.text_enc_ort.run(ort::inputs!{
            "text_ids" => TensorRef::from_array_view(text_ids_array.view())?,
            "style_ttl" => TensorRef::from_array_view(style.ttl.view())?,
            "text_mask" => TensorRef::from_array_view(text_mask.view())?,
        })?;

        let (text_emb_shape, text_emb_data) = text_enc_outputs["text_emb"].try_extract_tensor::<f32>()?;
        let text_emb = Array3::from_shape_vec(
            (text_emb_shape[0] as usize, text_emb_shape[1] as usize, text_emb_shape[2] as usize),
            text_emb_data.to_vec()
        )?;
        drop(text_enc_outputs);

        let (mut xt, latent_mask) = sample_noisy_latent(
            &duration, self.sample_rate, self.cfgs.ae.base_chunk_size,
            self.cfgs.ttl.chunk_compress_factor, self.cfgs.ttl.latent_dim,
        );

        let total_step_array = Array::from_elem((bsz,), total_step as f32);

        for step in 0..total_step {
            let current_step_array = Array::from_elem((bsz,), step as f32);

            let vector_est_outputs = self.vector_est_ort.run(ort::inputs!{
                "noisy_latent" => TensorRef::from_array_view(xt.view())?,
                "text_emb" => TensorRef::from_array_view(text_emb.view())?,
                "style_ttl" => TensorRef::from_array_view(style.ttl.view())?,
                "latent_mask" => TensorRef::from_array_view(latent_mask.view())?,
                "text_mask" => TensorRef::from_array_view(text_mask.view())?,
                "current_step" => Tensor::from_array(current_step_array)?,
                "total_step" => TensorRef::from_array_view(total_step_array.view())?,
            })?;

            let (denoised_shape, denoised_data) = vector_est_outputs["denoised_latent"].try_extract_tensor::<f32>()?;
            xt = Array3::from_shape_vec(
                (denoised_shape[0] as usize, denoised_shape[1] as usize, denoised_shape[2] as usize),
                denoised_data.to_vec()
            )?;
        }

        let vocoder_outputs = self.vocoder_ort.run(ort::inputs!{
            "latent" => Tensor::from_array(xt)?,
        })?;

        let (_, wav_data) = vocoder_outputs["wav_tts"].try_extract_tensor::<f32>()?;
        let wav: Vec<f32> = wav_data.to_vec();
        Ok((wav, duration))
    }

    pub fn call(
        &mut self,
        text: &str,
        lang: &str,
        style: &Style,
        total_step: usize,
        speed: f32,
        silence_duration: f32,
    ) -> Result<(Vec<f32>, f32)> {
        let sample_rate = self.sample_rate as f32;
        let mut wav_cat: Vec<f32> = Vec::new();
        let mut dur_cat: f32 = 0.0;
        self.call_streaming(text, lang, style, total_step, speed, silence_duration, |chunk, dur, idx| {
            if idx > 0 {
                let silence_len = (silence_duration * sample_rate) as usize;
                wav_cat.extend(std::iter::repeat(0.0f32).take(silence_len));
                dur_cat += silence_duration;
            }
            wav_cat.extend_from_slice(chunk);
            dur_cat += dur;
            Ok(())
        })?;
        Ok((wav_cat, dur_cat))
    }

    /// Synthesize text chunk by chunk. `on_chunk(samples, duration_seconds, chunk_index)` is invoked
    /// as soon as each chunk's PCM is ready, so the caller can start playback before the whole
    /// text finishes. The closure should return `Err` to abort remaining chunks.
    pub fn call_streaming<F>(
        &mut self,
        text: &str,
        lang: &str,
        style: &Style,
        total_step: usize,
        speed: f32,
        _silence_duration: f32,
        mut on_chunk: F,
    ) -> Result<()>
    where
        F: FnMut(&[f32], f32, usize) -> Result<()>,
    {
        let max_len = if lang == "ko" || lang == "ja" { 120 } else { 300 };
        let chunks = chunk_text(text, Some(max_len));

        for (i, chunk) in chunks.iter().enumerate() {
            let (wav, duration) = self._infer(&[chunk.clone()], &[lang.to_string()], style, total_step, speed)?;
            let dur = duration[0];
            let wav_len = (self.sample_rate as f32 * dur) as usize;
            let wav_chunk = &wav[..wav_len.min(wav.len())];
            on_chunk(wav_chunk, dur, i)?;
        }
        Ok(())
    }
}

pub fn load_voice_style<P: AsRef<Path>>(voice_style_path: P) -> Result<Style> {
    let file = File::open(voice_style_path)?;
    let reader = BufReader::new(file);
    let data: VoiceStyleData = serde_json::from_reader(reader)?;

    let ttl = Array3::from_shape_vec(
        (data.style_ttl.dims[0] as usize, data.style_ttl.dims[1] as usize, data.style_ttl.dims[2] as usize),
        data.style_ttl.data.into_iter().flatten().flatten().collect(),
    )?;
    let dp = Array3::from_shape_vec(
        (data.style_dp.dims[0] as usize, data.style_dp.dims[1] as usize, data.style_dp.dims[2] as usize),
        data.style_dp.data.into_iter().flatten().flatten().collect(),
    )?;

    Ok(Style { ttl, dp })
}
