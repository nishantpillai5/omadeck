//! CPU, memory and battery readers for live keys (PR 9). Parsers are pure; the daemon's
//! watcher reads the files.

use std::fs;
use std::path::Path;

/// Jiffies from the `cpu ` line of /proc/stat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

/// The `cpu ` line of /proc/stat: user nice system idle iowait irq softirq steal [guest
/// guest_nice]. total = the first eight summed (guest is already inside user);
/// busy = total − idle − iowait.
pub fn parse_proc_stat(stat: &str) -> Option<CpuTimes> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let fields: Vec<u64> = line.split_whitespace().skip(1).take(8).map(|f| f.parse().ok()).collect::<Option<_>>()?;
    if fields.len() < 4 {
        return None;
    }
    let total: u64 = fields.iter().sum();
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    Some(CpuTimes { busy: total.saturating_sub(idle), total })
}

/// Δbusy·100 / Δtotal, rounded; 0 when Δtotal is 0.
pub fn cpu_percent(prev: CpuTimes, now: CpuTimes) -> u8 {
    let total = now.total.saturating_sub(prev.total);
    if total == 0 {
        return 0;
    }
    let busy = now.busy.saturating_sub(prev.busy).min(total);
    ((busy * 100 + total / 2) / total) as u8
}

/// Keeps the previous sample. The first sample only primes it (returns None).
#[derive(Debug, Default)]
pub struct CpuMeter {
    prev: Option<CpuTimes>,
}

impl CpuMeter {
    pub fn sample(&mut self, stat: &str) -> Option<u8> {
        let now = parse_proc_stat(stat)?;
        let prev = self.prev.replace(now)?;
        Some(cpu_percent(prev, now))
    }
}

/// (MemTotal − MemAvailable) · 100 / MemTotal, rounded; None if either is missing.
pub fn parse_meminfo(meminfo: &str) -> Option<u8> {
    let field = |name: &str| -> Option<u64> {
        let line = meminfo.lines().find(|l| l.starts_with(name) && l[name.len()..].starts_with(':'))?;
        line[name.len() + 1..].split_whitespace().next()?.parse().ok()
    };
    let total = field("MemTotal")?;
    let available = field("MemAvailable")?;
    if total == 0 {
        return None;
    }
    let used = total.saturating_sub(available);
    Some(((used * 100 + total / 2) / total) as u8)
}

fn read(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name)).ok().map(|s| s.trim().to_string())
}

fn read_u64(dir: &Path, name: &str) -> Option<u64> {
    read(dir, name)?.parse().ok()
}

/// Battery % and charging from a power_supply directory. Entries with `type` = Battery
/// and no `scope` = Device (that's a mouse or headset). Several batteries combine as
/// Σenergy_now / Σenergy_full (else Σcharge_now / Σcharge_full), else the mean
/// `capacity`. charging = any `status` is "Charging". No battery → None.
pub fn battery_in(root: &Path) -> Option<(u8, bool)> {
    let mut dirs: Vec<_> = fs::read_dir(root).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    dirs.sort();
    let bats: Vec<_> = dirs
        .into_iter()
        .filter(|d| read(d, "type").as_deref() == Some("Battery") && read(d, "scope").as_deref() != Some("Device"))
        .collect();
    if bats.is_empty() {
        return None;
    }
    let charging = bats.iter().any(|d| read(d, "status").as_deref() == Some("Charging"));
    let ratio = |now: &str, full: &str| -> Option<u8> {
        let (mut n, mut f) = (0u64, 0u64);
        for d in &bats {
            n += read_u64(d, now)?;
            f += read_u64(d, full)?;
        }
        (f > 0).then(|| ((n * 100 + f / 2) / f).min(100) as u8)
    };
    let percent = ratio("energy_now", "energy_full").or_else(|| ratio("charge_now", "charge_full")).or_else(|| {
        let caps: Vec<u64> = bats.iter().filter_map(|d| read_u64(d, "capacity")).collect();
        let n = caps.len() as u64;
        (n > 0).then(|| ((caps.iter().sum::<u64>() + n / 2) / n).min(100) as u8)
    })?;
    Some((percent, charging))
}

/// The system battery, from /sys/class/power_supply.
pub fn battery() -> Option<(u8, bool)> {
    battery_in(Path::new("/sys/class/power_supply"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const STAT_A: &str = "cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 50 0 50 350 50 0 0 0 0 0\n";
    const STAT_B: &str = "cpu  150 0 150 750 150 0 0 0 0 0\ncpu0 75 0 75 375 75 0 0 0 0 0\n";

    #[test]
    fn cpu_percent_from_two_samples() {
        let a = parse_proc_stat(STAT_A).unwrap();
        let b = parse_proc_stat(STAT_B).unwrap();
        assert_eq!(a, CpuTimes { busy: 200, total: 1000 });
        // Δtotal 200, Δbusy 100.
        assert_eq!(cpu_percent(a, b), 50);
    }

    #[test]
    fn cpu_no_delta_is_zero() {
        let a = parse_proc_stat(STAT_A).unwrap();
        assert_eq!(cpu_percent(a, a), 0);
    }

    #[test]
    fn cpu_meter_primes_first() {
        let mut m = CpuMeter::default();
        assert_eq!(m.sample(STAT_A), None);
        assert_eq!(m.sample(STAT_B), Some(50));
    }

    #[test]
    fn meminfo_used_percent() {
        let m = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    4000000 kB\n";
        assert_eq!(parse_meminfo(m), Some(75));
    }

    #[test]
    fn meminfo_missing_field_is_none() {
        assert_eq!(parse_meminfo("MemTotal:       16000000 kB\nMemFree: 1 kB\n"), None);
    }

    struct Supply<'a> {
        name: &'a str,
        kind: &'a str,
        scope: Option<&'a str>,
        status: &'a str,
        capacity: u64,
        energy: Option<(u64, u64)>,
    }

    fn supplies(name: &str, list: &[Supply]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("omadeck-bat-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for s in list {
            let d = root.join(s.name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("type"), format!("{}\n", s.kind)).unwrap();
            if let Some(scope) = s.scope {
                fs::write(d.join("scope"), format!("{scope}\n")).unwrap();
            }
            fs::write(d.join("status"), format!("{}\n", s.status)).unwrap();
            fs::write(d.join("capacity"), format!("{}\n", s.capacity)).unwrap();
            if let Some((now, full)) = s.energy {
                fs::write(d.join("energy_now"), format!("{now}\n")).unwrap();
                fs::write(d.join("energy_full"), format!("{full}\n")).unwrap();
            }
        }
        root
    }

    fn bat<'a>(name: &'a str, status: &'a str, capacity: u64) -> Supply<'a> {
        Supply { name, kind: "Battery", scope: None, status, capacity, energy: None }
    }

    #[test]
    fn battery_charging_status() {
        for (status, charging) in [("Charging", true), ("Full", false), ("Discharging", false), ("Not charging", false)]
        {
            let root = supplies(&format!("status-{}", status.replace(' ', "-")), &[bat("BAT0", status, 64)]);
            assert_eq!(battery_in(&root), Some((64, charging)), "{status}");
            let _ = fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn battery_skips_device_scope() {
        let mouse = Supply { scope: Some("Device"), ..bat("hidpp_battery_0", "Discharging", 10) };
        let ac = Supply { kind: "Mains", ..bat("AC", "", 0) };
        let root = supplies("scope", &[mouse, ac, bat("BAT0", "Discharging", 80)]);
        assert_eq!(battery_in(&root), Some((80, false)));
        let _ = fs::remove_dir_all(&root);
        let root = supplies("scope-none", &[Supply { scope: Some("Device"), ..bat("hid", "Charging", 10) }]);
        assert_eq!(battery_in(&root), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn battery_combines_two() {
        let a = Supply { energy: Some((10_000, 50_000)), ..bat("BAT0", "Discharging", 20) };
        let b = Supply { energy: Some((40_000, 50_000)), ..bat("BAT1", "Charging", 80) };
        let root = supplies("two", &[a, b]);
        assert_eq!(battery_in(&root), Some((50, true)));
        let _ = fs::remove_dir_all(&root);
    }
}
