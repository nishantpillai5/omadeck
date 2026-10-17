//! The media player (MPRIS through `playerctl`): reading it, and the commands that drive it.
//!
//! Play/pause, next and previous go through Omarchy's own media commands when they're
//! there, so its pop-up shows as for the keyboard's media keys. Seeking, stop, a chosen
//! player and reading the state go through `playerctl`.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::audio::Backend;
use crate::config::MediaCommand;
use crate::render::MediaShow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayStatus {
    Playing,
    Paused,
    Stopped,
}

/// One player's state, as read by `playerctl`. Serde because `probe::Value` has it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// The instance, for `--player` (e.g. `chromium.instance4821`).
    pub player: String,
    /// The player's name (e.g. `chromium`, `spotify`), for `wanted` and the `playerctld` skip.
    pub name: String,
    pub status: PlayStatus,
    pub position_ms: u64,
    /// `None` for a live stream: the bar is left empty and the time shows elapsed only.
    pub length_ms: Option<u64>,
    pub title: String,
    pub artist: String,
    pub art: Option<PathBuf>,
    /// Decoded by the watcher; never serialised.
    #[serde(skip)]
    pub art_img: Option<Arc<RgbaImage>>,
    /// When `position_ms` was true (Unix ms).
    pub read_ms: u64,
}

impl Track {
    /// The panel model, anchored at the read.
    pub fn show(&self) -> MediaShow {
        MediaShow {
            title: self.title.clone(),
            artist: self.artist.clone(),
            playing: self.status == PlayStatus::Playing,
            position_ms: self.position_ms,
            length_ms: self.length_ms,
            as_of_ms: self.read_ms,
            art: self.art.clone(),
            art_img: self.art_img.clone(),
            note: String::new(),
        }
    }
}

/// What a read gives: a track, nothing playing, or why it couldn't read.
pub type PlayerRead = Result<Option<Track>, String>;

/// The field separator in [`FORMAT`]: the ASCII unit separator, which no title contains.
pub const SEP: char = '\x1f';
/// `playerctl -a metadata --format`: one line per player, eight fields.
pub const FORMAT: &str = "{{playerInstance}}\x1f{{playerName}}\x1f{{status}}\x1f{{position}}\x1f{{mpris:length}}\x1f{{xesam:title}}\x1f{{xesam:artist}}\x1f{{mpris:artUrl}}";
/// A read within this of the predicted position is "the same" (no seek happened).
pub const DRIFT_MS: u64 = 1500;
/// The largest seek one event sends, either way.
pub const MAX_SEEK_S: i32 = 600;

/// The wall-clock time in Unix ms, 0 if the clock is before 1970.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Parse `playerctl -a metadata --format FORMAT`. Lines that don't have exactly eight
/// fields or a known status are skipped. Positions are microseconds.
pub fn parse_playerctl(out: &str, read_ms: u64) -> Vec<Track> {
    out.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(SEP).collect();
            if f.len() != 8 {
                return None;
            }
            let status = match f[2].trim() {
                "Playing" => PlayStatus::Playing,
                "Paused" => PlayStatus::Paused,
                "Stopped" => PlayStatus::Stopped,
                _ => return None,
            };
            let us = |s: &str| s.trim().parse::<u64>().ok();
            Some(Track {
                player: f[0].trim().to_string(),
                name: f[1].trim().to_string(),
                status,
                position_ms: us(f[3]).unwrap_or(0) / 1000,
                length_ms: us(f[4]).filter(|&l| l > 0).map(|l| l / 1000),
                title: f[5].trim().to_string(),
                artist: f[6].trim().to_string(),
                art: art_path(f[7].trim()),
                art_img: None,
                read_ms,
            })
        })
        .collect()
}

/// The track to show: `playerctld` (a proxy) is skipped; with `wanted`, only that player
/// (by name, or an instance that starts with it). Then the first playing one, else the
/// first paused one with a title, else none.
pub fn pick(tracks: Vec<Track>, wanted: Option<&str>) -> Option<Track> {
    let mut ts: Vec<Track> = tracks
        .into_iter()
        .filter(|t| t.name != "playerctld")
        .filter(|t| wanted.is_none_or(|w| t.name == w || t.player.starts_with(w)))
        .collect();
    if let Some(i) = ts.iter().position(|t| t.status == PlayStatus::Playing) {
        return Some(ts.swap_remove(i));
    }
    let i = ts.iter().position(|t| t.status == PlayStatus::Paused && !t.title.is_empty())?;
    Some(ts.swap_remove(i))
}

/// A cover from `mpris:artUrl`: local `file://` only, percent-decoded, and only when the
/// file is there. The daemon never fetches from the network.
pub fn art_path(url: &str) -> Option<PathBuf> {
    let raw = url.strip_prefix("file://")?;
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let hex = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let path = PathBuf::from(OsString::from_vec(out));
    path.is_file().then_some(path)
}

/// Whether `new` says the same as `old`: same player, status, title, artist, length and
/// art, and the position where `old` predicts it (within `DRIFT_MS`). A track that simply
/// plays on is not a change; a seek, a pause or a new track is.
pub fn same(old: Option<&Track>, new: Option<&Track>) -> bool {
    match (old, new) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.player == b.player
                && a.status == b.status
                && a.title == b.title
                && a.artist == b.artist
                && a.length_ms == b.length_ms
                && a.art == b.art
                && b.position_ms.abs_diff(a.show().position_at(b.read_ms)) <= DRIFT_MS
        }
        _ => false,
    }
}

/// `0:07`, `1:23`, `12:05`, `1:02:03`.
pub fn fmt_time(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// One event's seek in seconds: `seek` per detent times `steps`, at most `MAX_SEEK_S`
/// either way.
pub fn fold_seek(seek: i16, steps: u8) -> i32 {
    (i32::from(seek) * i32::from(steps)).clamp(-MAX_SEEK_S, MAX_SEEK_S)
}

/// Whether this command needs `playerctl` (seek, stop, a target, or no Omarchy backend).
pub fn needs_playerctl(backend: Backend, cmd: MediaCommand, seek: i32, targeted: bool) -> bool {
    seek != 0 || cmd == MediaCommand::Stop || targeted || backend == Backend::Plain
}

/// The commands for one media action, in order. `seek` (seconds, signed) wins over `cmd`.
/// A `target` sends everything to `playerctl --player=<target>`, because Omarchy's
/// commands can't pick a player. `monitor` is for Omarchy 3's pop-up.
pub fn media_argv(
    backend: Backend,
    cmd: MediaCommand,
    seek: i32,
    target: Option<&str>,
    monitor: Option<&str>,
) -> Vec<Vec<String>> {
    let pc = |args: &[&str]| -> Vec<String> {
        let mut v = vec!["playerctl".to_string()];
        if let Some(t) = target {
            v.push(format!("--player={t}"));
        }
        v.extend(args.iter().map(|a| a.to_string()));
        v
    };
    if seek != 0 {
        let arg = format!("{}{}", seek.unsigned_abs(), if seek > 0 { "+" } else { "-" });
        return vec![pc(&["position", &arg])];
    }
    let (omarchy4, generic) = match cmd {
        MediaCommand::Stop => return vec![pc(&["stop"])],
        MediaCommand::PlayPause => ("playPause", "play-pause"),
        MediaCommand::Next => ("next", "next"),
        MediaCommand::Previous => ("previous", "previous"),
    };
    if target.is_some() {
        return vec![pc(&[generic])];
    }
    let argv: Vec<String> = match backend {
        Backend::Omarchy4 => vec!["omarchy-shell".into(), "media".into(), omarchy4.into()],
        Backend::Omarchy3 => {
            let mut v = vec!["swayosd-client".to_string()];
            if let Some(m) = monitor {
                v.extend(["--monitor".to_string(), m.to_string()]);
            }
            v.extend(["--playerctl".to_string(), generic.to_string()]);
            v
        }
        Backend::Plain => vec!["playerctl".into(), generic.into()],
    };
    vec![argv]
}

/// Run one command and wait for it (bounded by `COMMAND_TIMEOUT`), output discarded.
/// Returns whether it exited 0. Blocking: the watcher and the `media` thread only.
pub fn run_argv(argv: &[String]) -> bool {
    let Some((bin, args)) = argv.split_first() else { return false };
    let mut c = Command::new(bin);
    c.args(args).stdin(Stdio::null());
    match crate::proc::output_timeout(&mut c, crate::proc::COMMAND_TIMEOUT) {
        Some(out) if out.status.success() => true,
        Some(out) => {
            log::warn!("{argv:?} exited with {}", out.status);
            false
        }
        None => {
            log::warn!("{argv:?} could not run or timed out");
            false
        }
    }
}

/// Read every player once and pick the one to show. Blocking (bounded by
/// `COMMAND_TIMEOUT`): the watcher only. Art is not decoded here.
pub fn read(wanted: Option<&str>) -> PlayerRead {
    let mut cmd = Command::new("playerctl");
    cmd.args(["-a", "metadata", "--format", FORMAT]).env("LC_ALL", "C").stdin(Stdio::null());
    let Some(out) = crate::proc::output_timeout(&mut cmd, crate::proc::COMMAND_TIMEOUT) else {
        if !crate::action::which("playerctl") {
            return Err("Needs playerctl".into());
        }
        return Ok(None);
    };
    if !out.status.success() {
        // "No players found".
        return Ok(None);
    }
    Ok(pick(parse_playerctl(&String::from_utf8_lossy(&out.stdout), now_ms()), wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(player: &str, name: &str, status: PlayStatus, title: &str) -> Track {
        Track {
            player: player.into(),
            name: name.into(),
            status,
            position_ms: 0,
            length_ms: None,
            title: title.into(),
            artist: String::new(),
            art: None,
            art_img: None,
            read_ms: 0,
        }
    }

    fn line(fields: [&str; 8]) -> String {
        fields.join("\x1f")
    }

    #[test]
    fn same_on_tick_not_on_seek() {
        let t = |pos: u64, read: u64, status| Track {
            player: "spotify".into(),
            name: "spotify".into(),
            status,
            position_ms: pos,
            length_ms: Some(200_000),
            title: "Song".into(),
            artist: "Band".into(),
            art: None,
            art_img: None,
            read_ms: read,
        };
        let old = t(10_000, 1_000_000, PlayStatus::Playing);
        // One second later, one second further: just playing.
        assert!(same(Some(&old), Some(&t(11_000, 1_001_000, PlayStatus::Playing))));
        // Within the drift allowance (players report coarse positions).
        assert!(same(Some(&old), Some(&t(12_400, 1_001_000, PlayStatus::Playing))));
        // A seek from the player's own UI.
        assert!(!same(Some(&old), Some(&t(60_000, 1_001_000, PlayStatus::Playing))));
        // Paused: the position must not move.
        let paused = t(10_000, 1_000_000, PlayStatus::Paused);
        assert!(same(Some(&paused), Some(&t(10_000, 1_005_000, PlayStatus::Paused))));
        assert!(!same(Some(&old), Some(&paused)));
        assert!(same(None, None));
        assert!(!same(Some(&old), None));
    }

    // Captured on the Omarchy machine, 2026-10-17 (pr14-media-dial.md, Capture 4): a
    // browser player with `file://` art and a local-file player with none. The positions
    // are a snapshot of the capture.
    #[test]
    fn parse_playerctl_two_players() {
        let ts = parse_playerctl(include_str!("../tests/fixtures/playerctl-two.txt"), 7);
        assert_eq!(ts.len(), 2);
        assert_eq!(
            (ts[0].player.as_str(), ts[0].name.as_str(), ts[0].status),
            ("brave.instance3602", "brave", PlayStatus::Playing)
        );
        assert_eq!((ts[0].position_ms, ts[0].length_ms), (6_400_114, Some(9_509_061)));
        assert_eq!(ts[0].title.as_str(), "Bhagam Bhag 2006 (HD) - Full Movie - Superhit Comedy Movie - Akshay Kumar - Govinda -  Paresh Rawal");
        assert_eq!(ts[0].artist.as_str(), "Shemaroo Comedy");
        // The `file://` cover resolves where the file exists (the capturing machine) and
        // is dropped where it doesn't (CI).
        assert!(ts[0].art.as_deref().is_none_or(|p| p.to_string_lossy().ends_with(".org.chromium.Chromium.W1WPyo")));
        assert_eq!(art_path("https://i.scdn.co/image/ab67616d0000b273"), None, "https art is never fetched");
        assert_eq!((ts[1].player.as_str(), ts[1].name.as_str()), ("mpv", "mpv"));
        assert_eq!((ts[1].position_ms, ts[1].length_ms), (301, Some(1_088)));
        assert_eq!((ts[1].title.as_str(), ts[1].artist.as_str()), ("complete.oga", ""));
        assert_eq!(ts[1].art, None, "no art URL");
        assert_eq!(ts[1].read_ms, 7);
    }

    #[test]
    fn parse_playerctl_stream_without_length() {
        let out = line(["mpv", "mpv", "Playing", "5000000", "", "Groove Salad", "", ""]);
        let ts = parse_playerctl(&out, 0);
        assert_eq!(ts.len(), 1);
        assert_eq!((ts[0].position_ms, ts[0].length_ms), (5_000, None));
        let zero = line(["mpv", "mpv", "Playing", "5000000", "0", "Groove Salad", "", ""]);
        assert_eq!(parse_playerctl(&zero, 0)[0].length_ms, None);
    }

    #[test]
    fn parse_playerctl_garbage() {
        assert!(parse_playerctl("", 0).is_empty());
        assert!(parse_playerctl("a\x1fb", 0).is_empty());
        assert!(parse_playerctl("No players found", 0).is_empty());
        assert!(parse_playerctl(&line(["a", "a", "Buffering", "0", "0", "t", "a", ""]), 0).is_empty());
    }

    #[test]
    fn pick_prefers_playing() {
        let ts = vec![
            track("spotify", "spotify", PlayStatus::Paused, "Song"),
            track("chromium.instance1", "chromium", PlayStatus::Playing, "Video"),
        ];
        assert_eq!(pick(ts.clone(), None).unwrap().name, "chromium");
        let paused = vec![ts[0].clone(), track("mpv", "mpv", PlayStatus::Paused, "")];
        assert_eq!(pick(paused, None).unwrap().name, "spotify");
    }

    #[test]
    fn pick_wanted_by_name_or_instance() {
        let ts = vec![
            track("spotify", "spotify", PlayStatus::Paused, "Song"),
            track("chromium.instance1", "chromium", PlayStatus::Playing, "Video"),
        ];
        assert_eq!(pick(ts.clone(), Some("spotify")).unwrap().name, "spotify");
        assert_eq!(pick(ts.clone(), Some("chromium.instance1")).unwrap().name, "chromium");
        assert_eq!(pick(ts.clone(), Some("chromium")).unwrap().name, "chromium");
        assert!(pick(ts, Some("mpv")).is_none());
    }

    #[test]
    fn pick_skips_playerctld() {
        let ts = vec![
            track("playerctld", "playerctld", PlayStatus::Playing, "Proxy"),
            track("spotify", "spotify", PlayStatus::Paused, "Song"),
        ];
        assert_eq!(pick(ts, None).unwrap().name, "spotify");
    }

    #[test]
    fn pick_none_when_stopped() {
        let ts = vec![track("spotify", "spotify", PlayStatus::Stopped, "Song")];
        assert!(pick(ts, None).is_none());
        assert!(pick(vec![], None).is_none());
    }

    #[test]
    fn art_only_local_files() {
        let dir = std::env::temp_dir().join(format!("omadeck-art-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a b.png");
        std::fs::write(&file, b"x").unwrap();
        let url = format!("file://{}", file.display()).replace(' ', "%20");
        assert_eq!(art_path(&url), Some(file.clone()));
        assert_eq!(art_path("https://x/y.jpg"), None);
        assert_eq!(art_path("data:image/png;base64,AA"), None);
        assert_eq!(art_path(""), None);
        assert_eq!(art_path(&format!("file://{}", dir.join("missing.png").display())), None);
        assert_eq!(art_path("file://%zz"), None);
        assert_eq!(art_path("file://%2"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fmt_time_forms() {
        assert_eq!(fmt_time(7_000), "0:07");
        assert_eq!(fmt_time(83_000), "1:23");
        assert_eq!(fmt_time(725_000), "12:05");
        assert_eq!(fmt_time(3_723_000), "1:02:03");
    }

    #[test]
    fn panel_moves_while_playing() {
        let m = MediaShow { playing: true, position_ms: 10_000, as_of_ms: 1_000, ..Default::default() };
        assert_eq!(m.position_at(1_000), 10_000);
        assert_eq!(m.position_at(4_500), 13_500);
        assert_eq!(m.position_at(500), 10_000, "a clock behind the anchor doesn't go back");
    }

    #[test]
    fn panel_frozen_while_paused() {
        let m = MediaShow { playing: false, position_ms: 10_000, as_of_ms: 1_000, ..Default::default() };
        assert_eq!(m.position_at(60_000), 10_000);
    }

    #[test]
    fn panel_clamps_to_length() {
        let m = MediaShow {
            playing: true,
            position_ms: 10_000,
            as_of_ms: 0,
            length_ms: Some(11_000),
            ..Default::default()
        };
        assert_eq!(m.position_at(5_000), 11_000);
    }

    #[test]
    fn fold_seek_clamps() {
        assert_eq!(fold_seek(5, 3), 15);
        assert_eq!(fold_seek(-5, 1), -5);
        assert_eq!(fold_seek(-120, 8), -600);
        assert_eq!(fold_seek(i16::MAX, 8), 600);
    }

    #[test]
    fn media_argv_rows() {
        use Backend::*;
        use MediaCommand::*;
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<String>>();
        // Seeks and stop are always playerctl, untargeted or targeted.
        for b in [Omarchy4, Omarchy3, Plain] {
            assert_eq!(media_argv(b, PlayPause, 5, None, Some("DP-1")), vec![s(&["playerctl", "position", "5+"])]);
            assert_eq!(media_argv(b, Next, -5, None, None), vec![s(&["playerctl", "position", "5-"])]);
            assert_eq!(
                media_argv(b, PlayPause, -30, Some("spotify"), None),
                vec![s(&["playerctl", "--player=spotify", "position", "30-"])]
            );
            assert_eq!(media_argv(b, Stop, 0, None, None), vec![s(&["playerctl", "stop"])]);
            assert_eq!(media_argv(b, Stop, 0, Some("mpv"), None), vec![s(&["playerctl", "--player=mpv", "stop"])]);
            // A target sends play/pause, next and previous to playerctl too.
            assert_eq!(
                media_argv(b, PlayPause, 0, Some("spotify"), Some("DP-1")),
                vec![s(&["playerctl", "--player=spotify", "play-pause"])]
            );
            assert_eq!(
                media_argv(b, Next, 0, Some("spotify"), None),
                vec![s(&["playerctl", "--player=spotify", "next"])]
            );
            assert_eq!(
                media_argv(b, Previous, 0, Some("spotify"), None),
                vec![s(&["playerctl", "--player=spotify", "previous"])]
            );
        }
        assert_eq!(media_argv(Omarchy4, PlayPause, 0, None, None), vec![s(&["omarchy-shell", "media", "playPause"])]);
        assert_eq!(media_argv(Omarchy4, Next, 0, None, None), vec![s(&["omarchy-shell", "media", "next"])]);
        assert_eq!(media_argv(Omarchy4, Previous, 0, None, None), vec![s(&["omarchy-shell", "media", "previous"])]);
        assert_eq!(
            media_argv(Omarchy3, PlayPause, 0, None, Some("DP-1")),
            vec![s(&["swayosd-client", "--monitor", "DP-1", "--playerctl", "play-pause"])]
        );
        assert_eq!(media_argv(Omarchy3, Next, 0, None, None), vec![s(&["swayosd-client", "--playerctl", "next"])]);
        assert_eq!(
            media_argv(Omarchy3, Previous, 0, None, None),
            vec![s(&["swayosd-client", "--playerctl", "previous"])]
        );
        assert_eq!(media_argv(Plain, PlayPause, 0, None, None), vec![s(&["playerctl", "play-pause"])]);
        assert_eq!(media_argv(Plain, Next, 0, None, None), vec![s(&["playerctl", "next"])]);
        assert_eq!(media_argv(Plain, Previous, 0, None, None), vec![s(&["playerctl", "previous"])]);
    }

    #[test]
    fn needs_playerctl_rows() {
        use Backend::*;
        use MediaCommand::*;
        assert!(!needs_playerctl(Omarchy4, PlayPause, 0, false));
        assert!(!needs_playerctl(Omarchy3, Next, 0, false));
        assert!(!needs_playerctl(Omarchy4, Previous, 0, false));
        assert!(needs_playerctl(Plain, PlayPause, 0, false));
        assert!(needs_playerctl(Omarchy4, PlayPause, 5, false));
        assert!(needs_playerctl(Omarchy4, Stop, 0, false));
        assert!(needs_playerctl(Omarchy3, PlayPause, 0, true));
    }

    #[test]
    fn media_show_advances_from_as_of() {
        let m = MediaShow {
            playing: true,
            position_ms: 10_000,
            as_of_ms: 1_000,
            length_ms: Some(12_000),
            ..Default::default()
        };
        assert_eq!(m.at(3_000).position_ms, 12_000, "two seconds later, clamped to the length");
        assert_eq!(m.at(3_000).as_of_ms, 3_000);
        let paused = MediaShow { playing: false, ..m.clone() };
        assert_eq!(paused.at(9_000).position_ms, 10_000);
    }
}
