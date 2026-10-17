//! Real tool output from the Omarchy machine, through the real parsers.
//!
//! `scripts/capture-fixtures.sh` writes the files into `tests/fixtures/captured/<date>/`
//! (see the README there). Each test reads the newest folder that has its file; when no
//! folder has it, the test prints "fixture X not captured yet" and passes, so CI stays
//! green until the captures are committed.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use omadeck_core::audio::{parse_hyprctl_window, parse_pactl_streams, select_window};
use omadeck_core::follow::{CONFIGURATOR_CLASS, parse_clients};
use omadeck_core::player::{PlayStatus, parse_playerctl};
use omadeck_core::sysinfo::{battery_in, cpu_percent, parse_meminfo, parse_proc_stat};

/// `name` in the newest dated folder that has it, or `None` (and a note) when none does.
fn fixture_path(name: &str) -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/captured");
    let mut dirs: Vec<PathBuf> = fs::read_dir(&root)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    dirs.sort();
    let found = dirs.into_iter().rev().map(|d| d.join(name)).find(|p| p.exists());
    if found.is_none() {
        eprintln!("fixture {name} not captured yet (scripts/capture-fixtures.sh)");
    }
    found
}

fn fixture(name: &str) -> Option<String> {
    let p = fixture_path(name)?;
    Some(fs::read_to_string(&p).unwrap_or_else(|e| panic!("reading {}: {e}", p.display())))
}

/// Cases the script records `pactl -f json list sink-inputs` for.
const STREAM_CASES: [&str; 7] =
    ["spotify", "spotify-paused", "firefox-two-tabs", "chromium", "mpv-in-terminal", "flatpak", "none"];

#[test]
fn pactl_fixtures_parse() {
    for case in STREAM_CASES {
        let name = format!("sink-inputs-{case}.json");
        let Some(json) = fixture(&name) else { continue };
        let raw: serde_json::Value = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"));
        let streams = parse_pactl_streams(&json).unwrap_or_else(|| panic!("{name}: parse_pactl_streams gave None"));
        let total = raw.as_array().map_or(0, |a| a.len());
        assert_eq!(streams.len(), total, "{name}: a sink input was skipped (no usable object.id?)");
        for s in &streams {
            assert!(s.id > 0, "{name}: object.id 0: {s:?}");
            // 0..=1.5 as a fraction: pactl allows up to 150% by default.
            assert!(s.level.percent <= 150, "{name}: level above 150%: {s:?}");
        }
        if case == "none" {
            eprintln!("{name}: {} stream(s) with nothing playing", streams.len());
        } else {
            assert!(!streams.is_empty(), "{name}: no streams; was the app playing?");
            assert!(streams.iter().any(|s| s.pid.is_some()), "{name}: no stream has a pid: {streams:?}");
        }
    }
}

/// The focused window (and the process tree at that moment) finds the app's streams: by
/// pid or a descendant's (Chromium, mpv in a terminal), else by class (Flatpak).
#[test]
fn focused_window_finds_its_streams() {
    for case in STREAM_CASES.iter().filter(|c| !matches!(**c, "spotify-paused" | "none")) {
        let (Some(win), Some(tree), Some(json)) = (
            fixture(&format!("activewindow-{case}.json")),
            fixture(&format!("proc-tree-{case}.txt")),
            fixture(&format!("sink-inputs-{case}.json")),
        ) else {
            continue;
        };
        let window = parse_hyprctl_window(&win).unwrap_or_else(|| panic!("{case}: no focused window in the capture"));
        let parents: HashMap<u32, u32> = tree
            .lines()
            .filter_map(|l| {
                let mut f = l.split_whitespace().map(|n| n.parse::<u32>().ok());
                Some((f.next()??, f.next()??))
            })
            .collect();
        assert!(!parents.is_empty(), "{case}: empty process tree");
        let streams = parse_pactl_streams(&json).unwrap();
        let found = select_window(&streams, &window, |p| parents.get(&p).copied());
        assert!(
            !found.is_empty(),
            "{case}: the focused window {window:?} matched none of {streams:?} (was the app's window focused?)"
        );
    }
}

#[test]
fn hyprctl_window_fixtures_parse() {
    if let Some(json) = fixture("activewindow.json") {
        let w = parse_hyprctl_window(&json).expect("activewindow.json: a window was focused");
        assert!(!w.class.is_empty(), "activewindow.json: empty class");
        assert!(w.pid.is_some(), "activewindow.json: no pid: {w:?}");
    }
    if let Some(json) = fixture("activewindow-empty.json") {
        assert_eq!(parse_hyprctl_window(&json), None, "activewindow-empty.json: {json}");
    }
}

#[test]
fn configurator_class_is_the_constant() {
    let Some(json) = fixture("activewindow-configurator.json") else { return };
    let w = parse_hyprctl_window(&json).expect("the configurator was focused");
    assert_eq!(w.class, CONFIGURATOR_CLASS, "change follow::CONFIGURATOR_CLASS to the captured class");
}

#[test]
fn parse_clients_on_capture() {
    let Some(json) = fixture("hyprctl-clients.json") else { return };
    let raw: Vec<serde_json::Value> = serde_json::from_str(&json).expect("hyprctl-clients.json is a JSON array");
    assert!(!raw.is_empty(), "no clients captured");
    let got = parse_clients(&json);
    assert!(!got.is_empty(), "parse_clients found no classes in {} clients", raw.len());
    assert!(got.len() <= raw.len());
    assert!(got.iter().all(|(c, _)| !c.is_empty()));
    assert!(got.windows(2).all(|w| w[0].0 < w[1].0), "sorted and one per class: {got:?}");
    eprintln!("classes: {:?}", got.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>());
}

/// Every non-empty line is a track (eight fields, a known status).
fn tracks(name: &str, out: &str) -> Vec<omadeck_core::player::Track> {
    let ts = parse_playerctl(out, 1);
    let lines = out.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(ts.len(), lines, "{name}: a line didn't parse: {:?}", out.replace('\x1f', "␟"));
    for t in &ts {
        if let Some(len) = t.length_ms {
            // A little slack: the position and length are read at slightly different times.
            assert!(t.position_ms <= len + 5_000, "{name}: position past the length: {t:?}");
            assert!(len < 24 * 3_600_000, "{name}: length isn't microseconds? {t:?}");
        }
    }
    ts
}

#[test]
fn playerctl_two_players() {
    let Some(out) = fixture("playerctl-two.txt") else { return };
    let ts = tracks("playerctl-two.txt", &out);
    assert!(ts.len() >= 2, "expected two players: {ts:?}");
    assert!(ts.iter().any(|t| t.status == PlayStatus::Playing), "the browser was playing: {ts:?}");
    assert!(ts.iter().any(|t| t.status == PlayStatus::Paused), "Spotify was paused: {ts:?}");
}

#[test]
fn playerctl_stream_without_length() {
    let Some(out) = fixture("playerctl-stream.txt") else { return };
    let ts = tracks("playerctl-stream.txt", &out);
    assert!(ts.iter().any(|t| t.length_ms.is_none()), "no track without a length: {ts:?}");
}

#[test]
fn playerctl_no_players() {
    let Some(out) = fixture("playerctl-none.txt") else { return };
    assert!(parse_playerctl(&out, 0).is_empty(), "players with nothing open: {out:?}");
    if let Some(err) = fixture("playerctl-none.stderr.txt") {
        // `player::read` takes a non-zero exit, or no lines, as nothing playing.
        eprintln!("no players: {}", err.trim());
    }
}

#[test]
fn proc_samples_parse() {
    if let (Some(a), Some(b)) = (fixture("proc-stat-1.txt"), fixture("proc-stat-2.txt")) {
        let a = parse_proc_stat(&a).expect("proc-stat-1.txt parses");
        let b = parse_proc_stat(&b).expect("proc-stat-2.txt parses");
        assert!(b.total >= a.total && b.busy >= a.busy, "{a:?} then {b:?}");
        assert!(cpu_percent(a, b) <= 100);
    }
    if let Some(m) = fixture("meminfo.txt") {
        let used = parse_meminfo(&m).expect("meminfo.txt parses");
        assert!((1..=100).contains(&used), "memory {used}%");
    }
}

#[test]
fn battery_layout_reads() {
    for (dir, on_battery) in [("power_supply-battery", true), ("power_supply-ac", false)] {
        let Some(root) = fixture_path(dir) else { continue };
        let has_battery = fs::read_dir(&root)
            .map(|rd| {
                rd.filter_map(|e| e.ok()).any(|e| {
                    let read = |f: &str| fs::read_to_string(e.path().join(f)).unwrap_or_default();
                    read("type").trim() == "Battery" && read("scope").trim() != "Device"
                })
            })
            .unwrap_or(false);
        match battery_in(&root) {
            Some((percent, charging)) => {
                assert!(percent <= 100, "{dir}: {percent}%");
                if on_battery {
                    assert!(!charging, "{dir}: charging on battery");
                }
                eprintln!("{dir}: {percent}%, charging {charging}");
            }
            None => assert!(!has_battery, "{dir}: has a Battery but battery_in read nothing"),
        }
    }
}
