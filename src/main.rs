#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver};

use eframe::egui;
use genscribe::pipeline::{self, Input, Msg, Progress, Stage};
use genscribe::ytdlp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 520.0])
            .with_min_inner_size([480.0, 360.0])
            .with_title("genscribe"),
        ..Default::default()
    };
    eframe::run_native(
        "genscribe",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

enum UiState {
    Idle,
    Working { stage: Stage },
    Done { path: PathBuf, text: String },
    Error(String),
}

struct App {
    state: UiState,
    url_input: String,
    progress: Progress,
    rx: Option<Receiver<Msg>>,
    copied_flash: Option<std::time::Instant>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            state: UiState::Idle,
            url_input: String::new(),
            progress: Progress::new(),
            rx: None,
            copied_flash: None,
        }
    }
}

impl App {
    fn start(&mut self, input: Input) {
        self.progress.model_dl.store(0, Ordering::Relaxed);
        self.progress.url_dl.store(0, Ordering::Relaxed);
        self.progress.transcribe.store(0, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let initial_stage = match &input {
            Input::Url(_) => Stage::DownloadingUrl,
            Input::File(_) => Stage::Decoding,
        };
        self.state = UiState::Working {
            stage: initial_stage,
        };
        let progress = Progress {
            model_dl: self.progress.model_dl.clone(),
            url_dl: self.progress.url_dl.clone(),
            transcribe: self.progress.transcribe.clone(),
        };
        pipeline::spawn(input, progress, tx);
    }

    fn drain_messages(&mut self) {
        let Some(rx) = self.rx.as_ref() else { return };
        loop {
            match rx.try_recv() {
                Ok(Msg::Stage(s)) => self.state = UiState::Working { stage: s },
                Ok(Msg::ModelProgress)
                | Ok(Msg::UrlProgress)
                | Ok(Msg::TranscribeProgress) => {}
                Ok(Msg::Done { path, text }) => {
                    self.state = UiState::Done { path, text };
                    self.rx = None;
                    break;
                }
                Ok(Msg::Error(e)) => {
                    self.state = UiState::Error(e);
                    self.rx = None;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    break;
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();
        if self.rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        let is_working = matches!(self.state, UiState::Working { .. });
        if !is_working {
            let dropped: Vec<_> = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect()
            });
            if let Some(path) = dropped.into_iter().next() {
                if path.is_file() {
                    self.start(Input::File(path));
                } else {
                    self.state = UiState::Error("Dropped item is not a file".into());
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                ui.heading("genscribe");
                ui.label("Local Whisper transcription — drop an audio file or paste a URL.");
                ui.add_space(12.0);
            });

            match self.state {
                UiState::Idle => self.view_idle(ui),
                UiState::Working { ref stage } => {
                    let stage = stage.clone();
                    self.view_working(ui, stage);
                }
                UiState::Done { ref path, ref text } => {
                    let path = path.clone();
                    let text = text.clone();
                    self.view_done(ui, path, text);
                }
                UiState::Error(ref e) => {
                    let e = e.clone();
                    self.view_error(ui, e);
                }
            }

            // Drop-hover hint
            let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
            if hovering && !is_working {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("drop_overlay"),
                ));
                let screen = ctx.screen_rect();
                painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(160));
                painter.text(
                    screen.center(),
                    egui::Align2::CENTER_CENTER,
                    "Drop to transcribe",
                    egui::FontId::proportional(28.0),
                    egui::Color32::WHITE,
                );
            }
        });
    }
}

impl App {
    fn view_idle(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.set_min_height(120.0);
            ui.vertical_centered(|ui| {
                ui.add_space(28.0);
                ui.label(egui::RichText::new("⬇  Drop audio file here").size(18.0));
                ui.label("(mp3, m4a, wav, mp4, mov, …)");
                ui.add_space(8.0);
                if ui.button("Or choose a file…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(
                            "Audio / video",
                            &[
                                "mp3", "m4a", "wav", "aac", "flac", "ogg", "opus", "mp4", "mov",
                                "mkv", "webm",
                            ],
                        )
                        .pick_file()
                    {
                        self.start(Input::File(path));
                    }
                }
                ui.add_space(20.0);
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label("URL:");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.url_input)
                    .hint_text("https://youtube.com/…")
                    .desired_width(f32::INFINITY),
            );
            let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if submit {
                self.try_submit_url();
            }
        });
        ui.add_space(6.0);
        if ui.button("Transcribe URL").clicked() {
            self.try_submit_url();
        }
    }

    fn try_submit_url(&mut self) {
        let u = self.url_input.trim().to_string();
        if u.is_empty() {
            return;
        }
        if !ytdlp::is_url(&u) {
            self.state = UiState::Error("Not a valid http(s) URL".into());
            return;
        }
        self.url_input.clear();
        self.start(Input::Url(u));
    }

    fn view_working(&mut self, ui: &mut egui::Ui, stage: Stage) {
        self.draw_stage_strip(ui, &stage);
        ui.add_space(16.0);
        let (label, progress) = match stage {
            Stage::DownloadingModel => {
                let p = self.progress.model_dl.load(Ordering::Relaxed) as f32 / 100.0;
                ("Downloading Whisper model (first run)…", Some(p))
            }
            Stage::DownloadingUrl => {
                let p = self.progress.url_dl.load(Ordering::Relaxed) as f32 / 100.0;
                ("Downloading audio…", Some(p))
            }
            Stage::Decoding => ("Decoding audio…", None),
            Stage::Transcribing => {
                let p = self.progress.transcribe.load(Ordering::Relaxed) as f32 / 100.0;
                ("Transcribing…", Some(p))
            }
        };
        ui.label(label);
        ui.add_space(8.0);
        match progress {
            Some(p) => {
                ui.add(egui::ProgressBar::new(p).show_percentage().animate(true));
            }
            None => {
                ui.add(egui::Spinner::new().size(24.0));
            }
        }
    }

    fn draw_stage_strip(&self, ui: &mut egui::Ui, current: &Stage) {
        let steps = [
            ("Download", Stage::DownloadingUrl),
            ("Decode", Stage::Decoding),
            ("Transcribe", Stage::Transcribing),
            ("Save", Stage::Transcribing),
        ];
        let active_idx = match current {
            Stage::DownloadingModel | Stage::DownloadingUrl => 0,
            Stage::Decoding => 1,
            Stage::Transcribing => 2,
        };
        ui.horizontal(|ui| {
            for (i, (name, _)) in steps.iter().enumerate() {
                let color = if i < active_idx {
                    egui::Color32::from_rgb(120, 200, 120)
                } else if i == active_idx {
                    egui::Color32::from_rgb(120, 170, 240)
                } else {
                    egui::Color32::GRAY
                };
                ui.colored_label(color, format!("{}. {}", i + 1, name));
                if i < steps.len() - 1 {
                    ui.label("→");
                }
            }
        });
    }

    fn view_done(&mut self, ui: &mut egui::Ui, path: PathBuf, text: String) {
        ui.label(egui::RichText::new("Done").strong());
        ui.label(format!("Saved to: {}", path.display()));
        ui.add_space(8.0);

        ui.label("Preview:");
        let preview: String = text.chars().take(10_000).collect();
        let mut buf = preview;
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                ui.add_sized(
                    [ui.available_width(), 200.0],
                    egui::TextEdit::multiline(&mut buf)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            });

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Copy to clipboard").clicked() {
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text.clone())) {
                    Ok(_) => self.copied_flash = Some(std::time::Instant::now()),
                    Err(e) => self.state = UiState::Error(format!("Clipboard failed: {e}")),
                }
            }
            if ui.button("Save as…").clicked() {
                let default_name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "transcription.txt".into());
                if let Some(target) = rfd::FileDialog::new()
                    .set_file_name(&default_name)
                    .add_filter("Text", &["txt"])
                    .add_filter("Markdown", &["md"])
                    .save_file()
                {
                    if let Err(e) = std::fs::write(&target, &text) {
                        self.state = UiState::Error(format!("Save failed: {e}"));
                    }
                }
            }
            if ui.button("Reveal in Finder").clicked() {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(&path)
                    .spawn();
            }
            if ui.button("Open .txt").clicked() {
                let _ = std::process::Command::new("open").arg(&path).spawn();
            }
            if ui.button("Transcribe another").clicked() {
                self.state = UiState::Idle;
            }
        });

        if let Some(t) = self.copied_flash {
            if t.elapsed() < std::time::Duration::from_secs(2) {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 200, 120),
                    "Copied to clipboard ✓",
                );
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));
            } else {
                self.copied_flash = None;
            }
        }
    }

    fn view_error(&mut self, ui: &mut egui::Ui, err: String) {
        ui.add_space(12.0);
        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), "Error");
        ui.label(err);
        ui.add_space(12.0);
        if ui.button("Back").clicked() {
            self.state = UiState::Idle;
        }
    }
}
