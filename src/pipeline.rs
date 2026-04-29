use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::{audio, model, ytdlp};

#[derive(Debug, Clone)]
pub enum Stage {
    DownloadingModel,
    DownloadingUrl,
    Decoding,
    Transcribing,
}

#[derive(Debug)]
pub enum Msg {
    Stage(Stage),
    ModelProgress,
    UrlProgress,
    TranscribeProgress,
    Done { path: PathBuf, text: String },
    Error(String),
}

pub struct Progress {
    pub model_dl: Arc<AtomicU32>,
    pub url_dl: Arc<AtomicU32>,
    pub transcribe: Arc<AtomicU32>,
}

impl Progress {
    pub fn new() -> Self {
        Self {
            model_dl: Arc::new(AtomicU32::new(0)),
            url_dl: Arc::new(AtomicU32::new(0)),
            transcribe: Arc::new(AtomicU32::new(0)),
        }
    }
}

pub enum Input {
    File(PathBuf),
    Url(String),
}

pub fn spawn(input: Input, progress: Progress, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        if let Err(e) = run(input, progress, &tx) {
            let _ = tx.send(Msg::Error(format!("{e:#}")));
        }
    });
}

fn run(input: Input, progress: Progress, tx: &Sender<Msg>) -> Result<()> {
    // 1. Ensure model is available (download on first run)
    if !model::is_present() {
        tx.send(Msg::Stage(Stage::DownloadingModel)).ok();
        let p = progress.model_dl.clone();
        let tx2 = tx.clone();
        let stop = Arc::new(AtomicU32::new(0));
        let stop_clone = stop.clone();
        let ticker = std::thread::spawn(move || {
            while stop_clone.load(Ordering::Relaxed) == 0 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let _ = tx2.send(Msg::ModelProgress);
            }
        });
        let result = model::download(p);
        stop.store(1, Ordering::Relaxed);
        let _ = ticker.join();
        result?;
    }
    let model_path = model::model_path()?;

    // 2. Resolve input to a local audio file and choose output dir
    let tmp_dir = std::env::temp_dir().join("genscribe");
    std::fs::create_dir_all(&tmp_dir)?;

    let (source_audio, output_dir, stem) = match input {
        Input::File(p) => {
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "transcription".to_string());
            let dir = p
                .parent()
                .map(|x| x.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            (p, dir, stem)
        }
        Input::Url(u) => {
            tx.send(Msg::Stage(Stage::DownloadingUrl)).ok();
            let p = ytdlp::download_audio(&u, &tmp_dir, progress.url_dl.clone(), tx.clone())?;
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "transcription".to_string());
            let downloads = dirs::download_dir().unwrap_or_else(std::env::temp_dir);
            (p, downloads, stem)
        }
    };

    // 3. Decode to 16kHz mono wav
    tx.send(Msg::Stage(Stage::Decoding)).ok();
    let wav = audio::to_whisper_wav(&source_audio, &tmp_dir)?;
    let samples = audio::read_wav_f32(&wav)?;

    // 4. Transcribe
    tx.send(Msg::Stage(Stage::Transcribing)).ok();
    let text = transcribe(&model_path, &samples, progress.transcribe.clone(), tx.clone())?;

    // 5. Write output .txt (collision-safe)
    let out_path = unique_path(&output_dir, &stem, "txt");
    std::fs::write(&out_path, &text)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    tx.send(Msg::Done {
        path: out_path,
        text,
    })
    .ok();
    Ok(())
}

fn transcribe(
    model_path: &Path,
    samples: &[f32],
    progress: Arc<AtomicU32>,
    tx: Sender<Msg>,
) -> Result<String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let model_str = model_path
        .to_str()
        .context("model path is not valid utf-8")?;
    let ctx = WhisperContext::new_with_params(model_str, WhisperContextParameters::default())
        .context("failed to load whisper model")?;
    let mut state = ctx.create_state()?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_language(Some("en"));
    params.set_n_threads(num_threads());

    let prog = progress.clone();
    let tx_cb = tx.clone();
    params.set_progress_callback_safe(move |p: i32| {
        let v = p.clamp(0, 100) as u32;
        prog.store(v, Ordering::Relaxed);
        let _ = tx_cb.send(Msg::TranscribeProgress);
    });

    state.full(params, samples).context("whisper full() failed")?;

    let n = state.full_n_segments()?;
    let mut out = String::new();
    for i in 0..n {
        let seg = state.full_get_segment_text(i)?;
        out.push_str(seg.trim());
        out.push('\n');
    }
    progress.store(100, Ordering::Relaxed);
    Ok(out)
}

fn num_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4)
        .min(8)
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    candidate
}
