//! XDG locations used by omadeck.

use std::env;
use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

/// `~/.config/omadeck`
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("omadeck")
}

/// `~/.config/omadeck/config.toml`
pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// `~/.config/omadeck/icons` — icons imported through the configurator live here so the
/// whole config directory can be copied to another machine.
pub fn icons_dir() -> PathBuf {
    config_dir().join("icons")
}

/// Unix socket the daemon publishes events on: `$XDG_RUNTIME_DIR/omadeck.sock`.
///
/// There is deliberately no fallback: the runtime dir is private to the user (0700), while a
/// shared location such as `/tmp` would let other local users squat on or spoof the socket.
pub fn socket_path() -> anyhow::Result<PathBuf> {
    match env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir).join("omadeck.sock")),
        _ => anyhow::bail!("XDG_RUNTIME_DIR is not set; omadeck needs a private runtime directory for its socket"),
    }
}

/// Directory Omarchy keeps the active theme in.
pub fn omarchy_current_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("omarchy/current")
}

/// Expand a leading `~/` and resolve paths relative to the config dir.
pub fn resolve(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return home().join(rest);
    }
    let p = Path::new(path);
    if p.is_absolute() { p.to_path_buf() } else { config_dir().join(p) }
}

/// Inverse of [`resolve`]: store paths inside the config dir relatively, and paths under
/// `$HOME` with `~/`, so configs stay portable.
pub fn portable(path: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(config_dir()) {
        return rel.to_string_lossy().into_owned();
    }
    if let Ok(rel) = path.strip_prefix(home()) {
        return format!("~/{}", rel.to_string_lossy());
    }
    path.to_string_lossy().into_owned()
}

/// Identity and modification time of a file, following symlinks. Comparing two stamps tells
/// whether the file was edited or replaced, even when no filesystem event arrived (a symlink
/// to a file elsewhere, or a config directory that was deleted and recreated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
}

impl FileStamp {
    /// `None` when the file doesn't exist (or can't be read).
    pub fn of(path: &Path) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(path).ok()?;
        Some(Self { dev: m.dev(), ino: m.ino(), len: m.len(), mtime: (m.mtime(), m.mtime_nsec()) })
    }

    /// Whether both stamps are of the same file, edited or not.
    pub fn same_file(a: Option<Self>, b: Option<Self>) -> bool {
        a.map(|s| (s.dev, s.ino)) == b.map(|s| (s.dev, s.ino))
    }
}
