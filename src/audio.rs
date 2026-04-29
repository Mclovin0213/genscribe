use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::bin_resolve;

pub fn check_ffmpeg() -> Result<()> {
    Command::new(bin_resolve::resolve("ffmpeg"))
        .arg("-version")
        .output()
        .context("ffmpeg not found. The bundled copy may be missing or PATH lacks it.")?;
    Ok(())
}

/// Decode any audio/video file to 16kHz mono 16-bit WAV. Returns the wav path.
pub fn to_whisper_wav(input: &Path, tmp_dir: &Path) -> Result<PathBuf> {
    check_ffmpeg()?;
    std::fs::create_dir_all(tmp_dir)?;
    let out = tmp_dir.join("audio_16k_mono.wav");
    if out.exists() {
        let _ = std::fs::remove_file(&out);
    }

    let status = Command::new(bin_resolve::resolve("ffmpeg"))
        .args(["-y", "-i"])
        .arg(input)
        .args([
            "-ar", "16000", "-ac", "1", "-f", "wav", "-loglevel", "error",
        ])
        .arg(&out)
        .status()
        .context("failed to spawn ffmpeg")?;

    if !status.success() {
        bail!("ffmpeg exited with status {}", status);
    }
    Ok(out)
}

/// Read a 16kHz mono WAV file and return f32 samples in [-1.0, 1.0].
pub fn read_wav_f32(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("failed to open wav: {}", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        bail!("expected mono wav, got {} channels", spec.channels);
    }
    if spec.sample_rate != 16_000 {
        bail!("expected 16kHz wav, got {} Hz", spec.sample_rate);
    }

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max = (1i64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()?
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(samples)
}
