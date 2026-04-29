// Headless smoke test: runs the genscribe pipeline on a file path or URL,
// streams progress to stdout, and prints the output path + transcript head.
//
//   cargo run --release --example headless -- ./test_audio.mp3
//   cargo run --release --example headless -- https://www.youtube.com/watch?v=xTY3kPmDrOM

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;

use genscribe::pipeline::{self, Input, Msg, Progress, Stage};

fn main() {
    let arg = std::env::args().nth(1).expect("usage: headless <path-or-url>");
    let input = if arg.starts_with("http://") || arg.starts_with("https://") {
        Input::Url(arg)
    } else {
        Input::File(PathBuf::from(arg))
    };

    let progress = Progress::new();
    let mirror = Progress {
        model_dl: progress.model_dl.clone(),
        url_dl: progress.url_dl.clone(),
        transcribe: progress.transcribe.clone(),
    };
    let (tx, rx) = channel();
    pipeline::spawn(input, mirror, tx);

    let mut last_stage: Option<String> = None;
    let mut last_pct: i32 = -1;

    loop {
        match rx.recv() {
            Ok(Msg::Stage(s)) => {
                let name = stage_name(&s);
                if last_stage.as_deref() != Some(name) {
                    eprintln!("[stage] {name}");
                    last_stage = Some(name.into());
                    last_pct = -1;
                }
            }
            Ok(Msg::ModelProgress) => {
                let p = progress.model_dl.load(Ordering::Relaxed) as i32;
                tick(&mut last_pct, p, "model");
            }
            Ok(Msg::UrlProgress) => {
                let p = progress.url_dl.load(Ordering::Relaxed) as i32;
                tick(&mut last_pct, p, "url");
            }
            Ok(Msg::TranscribeProgress) => {
                let p = progress.transcribe.load(Ordering::Relaxed) as i32;
                tick(&mut last_pct, p, "transcribe");
            }
            Ok(Msg::Done { path, text }) => {
                println!("\nOK -> {}", path.display());
                let head: String = text.chars().take(400).collect();
                println!("--- transcript head ---\n{head}");
                if text.chars().count() > 400 {
                    println!("…(truncated)");
                }
                return;
            }
            Ok(Msg::Error(e)) => {
                eprintln!("ERROR: {e}");
                std::process::exit(1);
            }
            Err(_) => {
                eprintln!("channel closed");
                std::process::exit(2);
            }
        }
    }
}

fn stage_name(s: &Stage) -> &'static str {
    match s {
        Stage::DownloadingModel => "downloading-model",
        Stage::DownloadingUrl => "downloading-url",
        Stage::Decoding => "decoding",
        Stage::Transcribing => "transcribing",
    }
}

fn tick(last: &mut i32, now: i32, label: &str) {
    if now / 10 != *last / 10 {
        eprintln!("[{label}] {now}%");
        *last = now;
    }
}
