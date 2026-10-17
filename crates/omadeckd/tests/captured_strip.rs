//! What the Stream Deck + strip sends for a tap, a hold and a drifting hold, from the
//! daemon's own log lines (`strip tap at`, `strip long press at`, `strip swipe`),
//! captured by `scripts/capture-fixtures.sh` into `tests/fixtures/captured/<date>/`.
//! PR 16 assumes a hold is one `TouchScreenLongPress` with no tap before it. A missing
//! capture prints a note and passes.

use std::fs;
use std::path::{Path, PathBuf};

fn captured(name: &str) -> Option<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/captured");
    let mut dirs: Vec<PathBuf> =
        fs::read_dir(&root).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect()).unwrap_or_default();
    dirs.sort();
    let found = dirs.into_iter().rev().find_map(|d| fs::read_to_string(d.join(name)).ok());
    if found.is_none() {
        eprintln!("fixture {name} not captured yet (scripts/capture-fixtures.sh)");
    }
    found
}

fn count(log: &str, what: &str) -> usize {
    log.lines().filter(|l| l.contains(what)).count()
}

const TAP: &str = "strip tap at";
const LONG: &str = "strip long press at";
const SWIPE: &str = "strip swipe";

#[test]
fn a_tap_is_one_tap() {
    let Some(log) = captured("strip-tap.log") else { return };
    assert_eq!(count(&log, TAP), 1, "one tap, one `{TAP}`:\n{log}");
    assert_eq!(count(&log, LONG), 0, "a tap gave a long press:\n{log}");
}

#[test]
fn a_hold_is_only_a_long_press() {
    let Some(log) = captured("strip-hold.log") else { return };
    assert_eq!(count(&log, LONG), 1, "one hold, one `{LONG}`:\n{log}");
    assert_eq!(
        count(&log, TAP),
        0,
        "a hold also logged a tap, so it would run `press` and then `long_press` \
         (pr16-strip-long-press.md: stop and rethink):\n{log}"
    );
    // Lines are `<unix time> <log line>`; the header has when Enter was pressed.
    let enter = log.lines().find_map(|l| l.strip_prefix("# enter at ")?.split(';').next()?.parse::<f64>().ok());
    let at = log.lines().find(|l| l.contains(LONG)).and_then(|l| l.split_whitespace().next()?.parse::<f64>().ok());
    if let (Some(enter), Some(at)) = (enter, at) {
        eprintln!("long press logged {:.2} s after Enter (finger down plus the firmware's threshold)", at - enter);
    }
}

#[test]
fn a_drifting_hold() {
    let Some(log) = captured("strip-drift.log") else { return };
    let (tap, long, swipe) = (count(&log, TAP), count(&log, LONG), count(&log, SWIPE));
    eprintln!("drifting hold: {tap} tap, {long} long press, {swipe} swipe");
    assert!(tap + long + swipe > 0, "the drifting hold logged nothing:\n{log}");
    assert_eq!(tap, 0, "a drifting hold logged a tap:\n{log}");
}
