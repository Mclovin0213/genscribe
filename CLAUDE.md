# genscribe

Local Whisper transcription desktop app for **Apple Silicon Macs only** (arm64, macOS 11+). Drop a file or paste a URL → transcript. All processing runs locally.

## Architecture

Rust + egui (`eframe` 0.29). The app is both a binary and a library:

- `src/lib.rs` — re-exports modules so `examples/` and tests can use the pipeline.
- `src/main.rs` — egui `App` with `Idle` / `Working { stage }` / `Done { path, text }` / `Error` states. Drag-and-drop, URL field, "Choose a file…" via `rfd`, stage strip, progress bars, transcript preview, copy-to-clipboard (`arboard`), Save as… (`rfd`).
- `src/pipeline.rs` — runs on a worker thread, sends `Msg`s back. Stages: `DownloadingModel` → `DownloadingUrl` (URL only) → `Decoding` → `Transcribing`. `Msg::Done { path, text }` carries both so the GUI can preview/copy.
- `src/model.rs` — downloads `ggml-base.en.bin` (~150 MB) on first run to `~/Library/Application Support/genscribe/models/`. Reports progress via `Arc<AtomicU32>`.
- `src/audio.rs` — shells out to bundled `ffmpeg` to decode → 16 kHz mono WAV.
- `src/ytdlp.rs` — shells out to bundled `yt-dlp`. Streams `--newline --progress-template 'GS_PROGRESS %(progress._percent_str)s'` and `--print 'after_move:GS_FILE %(filepath)s'` to forward URL-download progress and resolve the output path. Output format is wav.
- `src/bin_resolve.rs` — `resolve(name)` looks up helper binaries in `<app>/Contents/Resources/bin/<name>` first (so the bundle is self-contained for non-technical users), falls back to `<exe_dir>/<name>`, then to `PATH`.

Whisper inference uses `whisper-rs` 0.13 with the `metal` feature (Apple GPU). `params.set_progress_callback_safe` drives the transcribe progress bar.

## Build / run / test

```bash
cargo build --release
./target/release/genscribe       # GUI

# Headless smoke test (used for CI / verification — no GUI):
cargo run --release --example headless -- ./test_audio.mp3
cargo run --release --example headless -- 'https://www.youtube.com/watch?v=xTY3kPmDrOM'
```

`examples/headless.rs` drives the same `pipeline::spawn` channel the GUI uses and prints stage/progress to stderr + final transcript head to stdout.

## Distribution: DMG + GitHub Releases

`macos/build_dmg.sh` builds an Apple-Silicon-only `.app` bundle and DMG:

1. `cargo build --release --target aarch64-apple-darwin`
2. Assembles `dist/genscribe.app/Contents/{MacOS, Resources/bin, Info.plist}`.
3. Copies `macos/Resources/bin/{ffmpeg,yt-dlp}` into the bundle (override paths via `GENSCRIBE_FFMPEG` / `GENSCRIBE_YTDLP` env vars). **Both must be arm64-only.**
4. Ad-hoc codesigns (`codesign --sign -`) — no Apple Developer account required, but users get a Gatekeeper warning on first launch (right-click → Open, or `xattr -dr com.apple.quarantine`).
5. `hdiutil` produces `dist/genscribe-<version>-arm64.dmg` with a drag-to-Applications symlink.

`Info.plist` uses `__VERSION__` placeholder, replaced at build time from `Cargo.toml`'s `version`.

`.github/workflows/release.yml` runs on `macos-14` (GitHub-hosted Apple Silicon runner). On `v*` tags it:
1. Fetches `yt-dlp_macos` (arm64) from yt-dlp's GitHub releases.
2. Fetches arm64 ffmpeg static build from `osxexperts.net`.
3. Runs `./macos/build_dmg.sh`.
4. Uploads DMG to the GitHub Release via `softprops/action-gh-release`.

## Conventions / gotchas

- **Apple Silicon only.** No Intel/`x86_64` build, no `lipo`/universal binary. `LSRequiresNativeExecution = true`. `build_dmg.sh` aborts on non-arm64 hosts.
- **No notarization.** README documents the right-click→Open / `xattr` workaround.
- **Helper binaries stay outside git.** `macos/Resources/bin/*` is gitignored except `.gitkeep`. The CI workflow downloads fresh copies; for local DMG builds, drop arm64 binaries into `macos/Resources/bin/` first or use the env-var overrides.
- **Homebrew `yt-dlp` is a Python wrapper script** — not self-contained. CI fetches the official `yt-dlp_macos` single-binary release asset.
- **Output paths.** File inputs → `.txt` next to source. URL inputs → `~/Downloads/<id>.txt`. Collisions handled by `unique_path()` (appends ` (1)`, ` (2)`, …).
- **Progress UX.** Determinate bars: model download, URL download, transcribe. Decode is a spinner (ffmpeg progress parsing not worth it; decode is fast).
