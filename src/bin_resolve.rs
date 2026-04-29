use std::path::PathBuf;

/// Resolve a helper binary name (e.g. "ffmpeg", "yt-dlp") to an absolute path.
///
/// Search order:
/// 1. `<bundle>/Contents/Resources/bin/<name>` — when running inside a .app bundle
/// 2. `<exe_dir>/<name>` — sibling to the executable (dev builds, plain binary)
/// 3. fall back to the bare name so `Command` searches `PATH`
pub fn resolve(name: &str) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos_dir) = exe.parent() {
            // .app/Contents/MacOS/<exe>  ->  .app/Contents/Resources/bin/<name>
            if let Some(contents) = macos_dir.parent() {
                let bundled = contents.join("Resources").join("bin").join(name);
                if bundled.is_file() {
                    return bundled;
                }
            }
            let sibling = macos_dir.join(name);
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from(name)
}
