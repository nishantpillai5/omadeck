//! Clock formatting and scheduling for live keys (PR 9).

use std::time::Duration;

/// Never sleep longer than this before re-checking the clock. Monotonic sleeps stop
/// during suspend, so an uncapped minute sleep would leave a stale time after resume.
pub const CLOCK_CAP: Duration = Duration::from_secs(5);

/// `now` formatted with `format`; "?" for a bad format.
pub fn format_clock(format: &str, now: &jiff::Zoned) -> String {
    jiff::fmt::strtime::format(format, now).unwrap_or_else(|_| "?".into())
}

/// The local time now, formatted. The watcher and the configurator's example row call
/// this, so neither needs jiff itself.
pub fn clock_text(format: &str) -> String {
    format_clock(format, &jiff::Zoned::now())
}

/// Unix time now, in milliseconds.
pub fn wall_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// Whether `format` shows seconds: %S %T %s %r %X %c %+, or fractions (%f %N), with
/// any of jiff's flags, width or precision (`%-S`, `%02S`, `%.3f`), but not `%%S`.
pub fn needs_seconds(format: &str) -> bool {
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        // Flags, width, precision and `%:z`'s colon come before the conversion
        // (BUGS.md 87). `%%` and every other conversion: skip it.
        while chars.next_if(|c| matches!(c, '-' | '_' | '0'..='9' | '^' | '#' | '.' | ':')).is_some() {}
        if matches!(chars.next(), Some('S' | 'T' | 's' | 'r' | 'X' | 'c' | '+' | 'f' | 'N')) {
            return true;
        }
    }
    false
}

/// Time from `now_ms` (Unix milliseconds) to just after the next minute (or second)
/// boundary, plus 10 ms so the format runs after the rollover; capped at `CLOCK_CAP`.
pub fn next_tick(now_ms: i64, seconds: bool) -> Duration {
    let period: i64 = if seconds { 1_000 } else { 60_000 };
    let to_next = period - now_ms.rem_euclid(period);
    Duration::from_millis((to_next + 10) as u64).min(CLOCK_CAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_formats_and_ticks() {
        let ts: jiff::Timestamp = "2026-10-15T12:00:59.5Z".parse().unwrap();
        let now = ts.to_zoned(jiff::tz::TimeZone::UTC);
        assert_eq!(format_clock("%H:%M", &now), "12:00");
        assert_eq!(format_clock("%H:%M%n%a %d", &now), "12:00\nThu 15");
        // 500 ms to the minute, plus the 10 ms margin.
        assert_eq!(next_tick(ts.as_millisecond(), false), Duration::from_millis(510));
        assert_eq!(next_tick(ts.as_millisecond(), true), Duration::from_millis(510));
        assert_eq!(next_tick(ts.as_millisecond() + 1, false), Duration::from_millis(509));
    }

    #[test]
    fn needs_seconds_formats() {
        assert!(!needs_seconds("%H:%M"));
        assert!(needs_seconds("%T"));
        assert!(needs_seconds("%H:%M:%S"));
        assert!(needs_seconds("%r"));
        assert!(!needs_seconds("%%S"));
        assert!(!needs_seconds("%a %d %b"));
        // BUGS.md 87: flags, width and fractions.
        assert!(needs_seconds("%H:%M:%-S"));
        assert!(needs_seconds("%_S"));
        assert!(needs_seconds("%02S"));
        assert!(needs_seconds("%^r"));
        assert!(needs_seconds("%T.%f"));
        assert!(needs_seconds("%.3f"));
        assert!(needs_seconds("%N"));
        assert!(!needs_seconds("%-H:%M %:z"));
        assert!(!needs_seconds("%%-S"));
    }

    #[test]
    fn next_tick_second_boundary() {
        assert_eq!(next_tick(1_000, true), Duration::from_millis(1_010));
        assert_eq!(next_tick(1_250, true), Duration::from_millis(760));
        // Negative times still land on the boundary.
        assert_eq!(next_tick(-250, true), Duration::from_millis(260));
    }

    #[test]
    fn next_tick_capped() {
        assert_eq!(next_tick(0, false), CLOCK_CAP);
        assert_eq!(next_tick(30_000, false), CLOCK_CAP);
    }

    #[test]
    fn bad_format_is_question_mark() {
        let now = jiff::Timestamp::UNIX_EPOCH.to_zoned(jiff::tz::TimeZone::UTC);
        assert_eq!(format_clock("%H:%", &now), "?");
    }
}
