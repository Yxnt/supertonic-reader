pub mod helper;

use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::Result;
use self::helper::{build_session, load_cfgs, SessionKind, UnicodeProcessor, TextToSpeech, Style, load_voice_style};

pub struct TtsEngine {
    pub tts: Mutex<TextToSpeech>,
    pub style: Mutex<Style>,
    pub sample_rate: i32,
}

impl TtsEngine {
    pub fn new(model_dir: &PathBuf, voice_style: &str) -> Result<Self> {
        let cfgs = load_cfgs(model_dir.join("onnx"))?;
        let text_processor = UnicodeProcessor::new(model_dir.join("onnx").join("unicode_indexer.json"))?;

        let dp_ort = build_session(model_dir.join("onnx").join("duration_predictor.onnx"), SessionKind::DurationPredictor)?;
        let text_enc_ort = build_session(model_dir.join("onnx").join("text_encoder.onnx"), SessionKind::TextEncoder)?;
        let vector_est_ort = build_session(model_dir.join("onnx").join("vector_estimator.onnx"), SessionKind::VectorEstimator)?;
        let vocoder_ort = build_session(model_dir.join("onnx").join("vocoder.onnx"), SessionKind::Vocoder)?;

        let tts = TextToSpeech::new(cfgs, text_processor, dp_ort, text_enc_ort, vector_est_ort, vocoder_ort);
        let sample_rate = tts.sample_rate;

        let style_path = model_dir.join("voice_styles").join(format!("{}.json", voice_style));
        let style = load_voice_style(&style_path)?;

        Ok(TtsEngine { tts: Mutex::new(tts), style: Mutex::new(style), sample_rate })
    }

    pub fn replace_style(&self, new_style: Style) -> Result<()> {
        let mut style = self.style.lock().map_err(|e| anyhow::anyhow!("Style lock: {}", e))?;
        *style = new_style;
        Ok(())
    }

    pub fn synthesize_with_style(&self, text: &str, lang: &str, steps: usize, speed: f32, silence: f32, style: &Style) -> Result<(Vec<f32>, f32)> {
        let mut tts = self.tts.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        tts.call(text, lang, style, steps, speed, silence)
    }

    /// Streaming variant of `synthesize_with_style`.
    pub fn synthesize_streaming_with_style<F>(
        &self,
        text: &str,
        lang: &str,
        steps: usize,
        speed: f32,
        silence: f32,
        style: &Style,
        on_chunk: F,
    ) -> Result<()>
    where
        F: FnMut(&[f32], f32, usize) -> Result<()>,
    {
        let mut tts = self.tts.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        tts.call_streaming(text, lang, style, steps, speed, silence, on_chunk)
    }

    /// Streaming variant using the engine's current preset style.
    pub fn synthesize_streaming<F>(
        &self,
        text: &str,
        lang: &str,
        steps: usize,
        speed: f32,
        silence: f32,
        on_chunk: F,
    ) -> Result<()>
    where
        F: FnMut(&[f32], f32, usize) -> Result<()>,
    {
        // Take a snapshot of the style under the style lock, then release it
        // before grabbing the tts lock so a voice-style hot-update can't
        // deadlock against a long-running synthesis.
        let style_snapshot = {
            let s = self.style.lock().map_err(|e| anyhow::anyhow!("Style lock: {}", e))?;
            Style { ttl: s.ttl.clone(), dp: s.dp.clone() }
        };
        let mut tts = self.tts.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        tts.call_streaming(text, lang, &style_snapshot, steps, speed, silence, on_chunk)
    }
}
