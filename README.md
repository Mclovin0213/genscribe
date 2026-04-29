# genscribe

Drop in an audio file (or paste a URL). Get a transcript. All on your Mac, offline after first run.

> **Apple Silicon Macs only** (M1 / M2 / M3 / M4, macOS 11 Big Sur or newer). Intel Macs are not supported.

---

## Install (non-technical)

1. Go to the [**Releases**](../../releases) page.
2. Download the latest **`genscribe-x.y.z-arm64.dmg`**.
3. Open the DMG, drag **genscribe** into your **Applications** folder.
4. The first time you launch it, **right-click** the app icon and pick **Open** → **Open** in the dialog.
   (macOS does this for apps that aren't from the App Store. After the first time you can double-click as normal.)

If macOS still complains ("damaged" / "can't be opened"), run this once in Terminal:

```bash
xattr -dr com.apple.quarantine /Applications/genscribe.app
```

That clears the quarantine flag — the app is unsigned (no Apple Developer account behind it) but is otherwise just a regular Mac app.

---

## How to use

- **A file:** drag any audio or video file (mp3, m4a, wav, mp4, mov, …) onto the window. Or click **Choose a file…**.
- **A URL:** paste a YouTube / podcast / etc. link and hit **Transcribe URL**.

When it's finished you'll see:

- A **preview** of the transcript.
- **Copy to clipboard** — paste the text anywhere.
- **Save as…** — pick where to save (`.txt` or `.md`).
- **Reveal in Finder** — opens the auto-saved `.txt` next to the source file (or in Downloads, for URL sources).
- **Open .txt** — opens the file in your default text editor.
- **Transcribe another** — go again.

### What's happening behind the scenes

1. **Download model** (only the very first time, ~150 MB — Whisper `base.en`). Stored in `~/Library/Application Support/genscribe/models/`.
2. **Download audio** (URL inputs only, via bundled `yt-dlp`).
3. **Decode** to 16 kHz mono WAV (via bundled `ffmpeg`).
4. **Transcribe** locally with Whisper on the Apple GPU (Metal).
5. **Save** as a `.txt` next to your source file.

Nothing is uploaded. Everything runs on your machine.

---

## Build from source

Apple Silicon Mac required.

```bash
cargo build --release
./target/release/genscribe       # GUI

# Or the headless smoke test:
cargo run --release --example headless -- ./test_audio.mp3
cargo run --release --example headless -- "https://www.youtube.com/watch?v=xTY3kPmDrOM"
```

To build a distributable DMG:

```bash
# Place arm64 binaries first:
#   macos/Resources/bin/ffmpeg
#   macos/Resources/bin/yt-dlp
# (or set GENSCRIBE_FFMPEG / GENSCRIBE_YTDLP env vars)
./macos/build_dmg.sh
# -> dist/genscribe-<version>-arm64.dmg
```

CI: pushing a tag matching `v*` triggers `.github/workflows/release.yml`, which builds on a `macos-14` runner (Apple Silicon) and uploads the DMG to the GitHub Release.

---

## Bundled third-party binaries

The DMG includes:

- [`ffmpeg`](https://ffmpeg.org/) (LGPL/GPL — see ffmpeg.org/legal.html)
- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) (Unlicense)

Sources are linked above; both are unmodified upstream binaries.
