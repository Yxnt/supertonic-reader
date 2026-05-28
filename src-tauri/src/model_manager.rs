use std::path::PathBuf;
use anyhow::{Result, anyhow};
use reqwest;
use futures_util::StreamExt;

const REQUIRED_FILES: &[&str] = &[
    "onnx/duration_predictor.onnx",
    "onnx/text_encoder.onnx",
    "onnx/vector_estimator.onnx",
    "onnx/vocoder.onnx",
    "onnx/unicode_indexer.json",
    "onnx/tts.json",
    "voice_styles/M1.json",
    "voice_styles/M2.json",
    "voice_styles/M3.json",
    "voice_styles/M4.json",
    "voice_styles/M5.json",
    "voice_styles/F1.json",
    "voice_styles/F2.json",
    "voice_styles/F3.json",
    "voice_styles/F4.json",
    "voice_styles/F5.json",
];

pub const HF_BASE_URL: &str = "https://huggingface.co/Supertone/supertonic-3/resolve/main";
pub const HF_MIRROR_URL: &str = "https://hf-mirror.com/Supertone/supertonic-3/resolve/main";

pub fn is_model_ready(model_dir: &PathBuf) -> bool {
    for file in REQUIRED_FILES {
        if !model_dir.join(file).exists() {
            return false;
        }
    }
    true
}

pub fn missing_files(model_dir: &PathBuf) -> Vec<String> {
    let mut missing = Vec::new();
    for file in REQUIRED_FILES {
        if !model_dir.join(file).exists() {
            missing.push(file.to_string());
        }
    }
    missing
}

pub async fn download_model(
    model_dir: &PathBuf,
    base_url: &str,
    progress_callback: impl Fn(u64, u64) + Send + 'static,
) -> Result<()> {
    let missing = missing_files(model_dir);
    if missing.is_empty() {
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let total_files = missing.len();

    for (i, file) in missing.iter().enumerate() {
        let url = format!("{}/{}", base_url, file);
        let dest = model_dir.join(file);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let response = client.get(&url).send().await
            .map_err(|e| anyhow!("请求 {} 失败: {}", file, e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("下载 {} 失败: HTTP {} - {}", file, status, body));
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut file_content = Vec::new();
        let mut last_progress: u64 = (i as u64 * 100 / total_files as u64).max(1);
        progress_callback(last_progress, total_files as u64);

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| anyhow!("下载 {} 流错误: {}", file, e))?;
            file_content.extend_from_slice(&chunk);
            downloaded += chunk.len() as u64;

            let file_progress = if total_size > 0 {
                downloaded * 100 / total_size
            } else {
                if downloaded > 0 { 50 } else { 0 }
            };
            let overall_progress = ((i as u64 * 100 + file_progress) / total_files as u64) as u64;
            let progress = overall_progress.max(last_progress).min(99);
            if progress > last_progress {
                last_progress = progress;
                progress_callback(progress, total_files as u64);
            }
        }

        if total_size > 0 && downloaded < total_size {
            let _ = std::fs::remove_file(&dest);
            return Err(anyhow!("下载 {} 不完整: {}/{} 字节", file, downloaded, total_size));
        }

        tokio::fs::write(&dest, file_content).await
            .map_err(|e| anyhow!("写入 {} 失败: {}", file, e))?;
    }

    progress_callback(100, total_files as u64);
    Ok(())
}
