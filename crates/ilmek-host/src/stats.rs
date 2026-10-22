//! Measured resource use. Values that cannot be measured on this platform are `None`, never 0.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppStats {
    /// Resident memory of the backend process (Linux only).
    pub rss_bytes: Option<u64>,
    /// CPU time consumed by the backend process so far (Linux only).
    pub cpu_ms: Option<u64>,
    /// Size of the application's data directory.
    pub data_bytes: u64,
}

#[must_use]
pub fn sample(pid: Option<u32>, data_dir: &Path) -> AppStats {
    let (rss_bytes, cpu_ms) = pid.map_or((None, None), process_usage);
    AppStats {
        rss_bytes,
        cpu_ms,
        data_bytes: directory_size(data_dir),
    }
}

#[cfg(target_os = "linux")]
fn process_usage(pid: u32) -> (Option<u64>, Option<u64>) {
    let rss = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let kilobytes = line
                    .strip_prefix("VmRSS:")?
                    .trim()
                    .strip_suffix("kB")?
                    .trim();
                kilobytes.parse::<u64>().ok().map(|kb| kb * 1024)
            })
        });
    let cpu = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            // Fields after the parenthesised command name; utime and stime are the 12th and 13th.
            let after_name = stat.rsplit_once(')')?.1;
            let mut fields = after_name.split_whitespace();
            let utime: u64 = fields.nth(11)?.parse().ok()?;
            let stime: u64 = fields.next()?.parse().ok()?;
            // Linux reports these in USER_HZ ticks, which is 100 on every supported architecture.
            Some((utime + stime) * 10)
        });
    (rss, cpu)
}

#[cfg(not(target_os = "linux"))]
fn process_usage(_pid: u32) -> (Option<u64>, Option<u64>) {
    (None, None)
}

fn directory_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => directory_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}
