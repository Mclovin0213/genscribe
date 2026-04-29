use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::bin_resolve;
use crate::pipeline::Msg;

pub fn is_url(s: &str) -> bool {
    url::Url::parse(s.trim())
        .map(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or(false)
}

pub fn check_installed() -> Result<()> {
    Command::new(bin_resolve::resolve("yt-dlp"))
        .arg("--version")
        .output()
        .context("yt-dlp not found. The bundled copy may be missing or PATH lacks it.")?;
    Ok(())
}

/// Downloads audio from a URL into `out_dir`. Reports progress via `progress` (0..=100).
/// Returns path to the downloaded audio file.
pub fn download_audio(
    url: &str,
    out_dir: &Path,
    progress: Arc<AtomicU32>,
    tx: Sender<Msg>,
) -> Result<PathBuf> {
    check_installed()?;
    std::fs::create_dir_all(out_dir)?;

    let template = out_dir.join("%(id)s.%(ext)s");
    let mut child = Command::new(bin_resolve::resolve("yt-dlp"))
        .args([
            "-x",
            "--audio-format",
            "wav",
            "--no-playlist",
            "--newline",
            "--progress-template",
            "GS_PROGRESS %(progress._percent_str)s",
            "--print",
            "after_move:GS_FILE %(filepath)s",
            "-o",
        ])
        .arg(&template)
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn yt-dlp")?;

    let stdout = child.stdout.take().context("yt-dlp stdout missing")?;
    let reader = BufReader::new(stdout);
    let mut filepath: Option<String> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if let Some(rest) = line.strip_prefix("GS_PROGRESS") {
            // e.g. "GS_PROGRESS  42.3%"
            let trimmed = rest.trim().trim_end_matches('%').trim();
            if let Ok(p) = trimmed.parse::<f32>() {
                progress.store(p.clamp(0.0, 100.0) as u32, Ordering::Relaxed);
                let _ = tx.send(Msg::UrlProgress);
            }
        } else if let Some(rest) = line.strip_prefix("GS_FILE ") {
            filepath = Some(rest.trim().to_string());
        }
    }

    let status = child.wait().context("waiting for yt-dlp")?;
    if !status.success() {
        bail!("yt-dlp exited with status {}", status);
    }

    let path_str = filepath.unwrap_or_default();
    if path_str.is_empty() {
        bail!("yt-dlp did not report an output path");
    }
    let p = PathBuf::from(path_str);
    if !p.exists() {
        bail!("yt-dlp output path does not exist: {}", p.display());
    }
    progress.store(100, Ordering::Relaxed);
    Ok(p)
}
