use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";
const MODEL_FILENAME: &str = "ggml-base.en.bin";
const MIN_MODEL_BYTES: u64 = 100 * 1024 * 1024;

pub fn model_path() -> Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .context("could not resolve local data dir")?
        .join("genscribe")
        .join("models");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(MODEL_FILENAME))
}

pub fn is_present() -> bool {
    match model_path() {
        Ok(p) => std::fs::metadata(&p)
            .map(|m| m.len() >= MIN_MODEL_BYTES)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Download the model. `progress` is filled with 0..=100.
pub fn download(progress: Arc<AtomicU32>) -> Result<PathBuf> {
    let dest = model_path()?;
    let tmp = dest.with_extension("bin.part");

    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()?;
    let mut resp = client
        .get(MODEL_URL)
        .send()
        .context("failed to start model download")?;
    if !resp.status().is_success() {
        bail!("model download HTTP {}", resp.status());
    }
    let total = resp.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&tmp)?;
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        if total > 0 {
            let pct = ((downloaded * 100) / total).min(100) as u32;
            progress.store(pct, Ordering::Relaxed);
        }
    }
    drop(file);

    if std::fs::metadata(&tmp)?.len() < MIN_MODEL_BYTES {
        let _ = std::fs::remove_file(&tmp);
        bail!("downloaded model is too small — aborting");
    }
    std::fs::rename(&tmp, &dest)?;
    progress.store(100, Ordering::Relaxed);
    Ok(dest)
}
