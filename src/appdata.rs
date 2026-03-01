use std::path::PathBuf;

/// Returns `%APPDATA%\WinTab`, creating the directory if needed.
pub fn config_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let dir = PathBuf::from(appdata).join("WinTab");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok()?;
    }
    Some(dir)
}
