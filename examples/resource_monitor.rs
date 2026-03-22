/// Resource usage monitor for wtop.
///
/// Run wtop in one terminal, then in another:
///
///   cargo run --example resource_monitor
///
/// Or supply a PID directly:
///
///   cargo run --example resource_monitor -- 12345
///
/// Environment variables:
///   DURATION_SECS   — how long to monitor (default: 300 = 5 minutes)
///   SAMPLE_SECS     — sampling interval  (default: 2)
///   LEAK_THRESHOLD  — RSS growth % to flag as leak (default: 25)
use std::fs;
use std::io::{self, BufRead};
use std::time::{Duration, Instant};
use std::thread;

fn main() {
    let pid = resolve_pid();
    let duration = env_or("DURATION_SECS", 300);
    let interval = env_or("SAMPLE_SECS", 2);
    let leak_threshold = env_or("LEAK_THRESHOLD", 25);

    eprintln!("Monitoring wtop (PID {}) for {}s, sampling every {}s", pid, duration, interval);
    eprintln!("Leak threshold: {}% RSS growth", leak_threshold);
    eprintln!("{:-<72}", "");
    eprintln!(
        "{:>6}  {:>12}  {:>12}  {:>12}  {:>8}",
        "Time", "RSS (KiB)", "RSS delta", "Peak RSS", "CPU %"
    );
    eprintln!("{:-<72}", "");

    let page_size = page_size_kb();
    let start = Instant::now();
    let mut samples: Vec<Sample> = Vec::new();
    let mut prev_cpu: Option<(u64, u64)> = None; // (utime+stime, total system jiffies)
    let mut peak_rss: u64 = 0;

    loop {
        let elapsed = start.elapsed();
        if elapsed >= Duration::from_secs(duration) {
            break;
        }

        // Check process is still alive
        let stat_path = format!("/proc/{}/stat", pid);
        if fs::metadata(&stat_path).is_err() {
            eprintln!("\nProcess {} exited after {:.1}s", pid, elapsed.as_secs_f64());
            break;
        }

        let rss_kb = read_rss(pid, page_size);
        let cpu_pct = read_cpu(pid, &mut prev_cpu);

        if rss_kb > peak_rss {
            peak_rss = rss_kb;
        }

        let initial_rss = samples.first().map(|s| s.rss_kb).unwrap_or(rss_kb);
        let delta: i64 = rss_kb as i64 - initial_rss as i64;

        samples.push(Sample {
            elapsed_secs: elapsed.as_secs(),
            rss_kb,
            cpu_pct,
        });

        let delta_str = if delta >= 0 {
            format!("+{}", delta)
        } else {
            format!("{}", delta)
        };

        eprintln!(
            "{:>5}s  {:>10} K  {:>10} K  {:>10} K  {:>7.1}",
            elapsed.as_secs(),
            rss_kb,
            delta_str,
            peak_rss,
            cpu_pct,
        );

        thread::sleep(Duration::from_secs(interval));
    }

    // --- Summary ---
    if samples.len() < 2 {
        eprintln!("\nNot enough samples to analyze.");
        return;
    }

    let initial = samples.first().unwrap();
    let final_s = samples.last().unwrap();
    let rss_growth = final_s.rss_kb as f64 - initial.rss_kb as f64;
    let growth_pct = if initial.rss_kb > 0 {
        (rss_growth / initial.rss_kb as f64) * 100.0
    } else {
        0.0
    };
    let avg_cpu: f64 = samples.iter().map(|s| s.cpu_pct).sum::<f64>() / samples.len() as f64;
    let max_cpu: f64 = samples.iter().map(|s| s.cpu_pct).fold(0.0f64, f64::max);

    eprintln!("\n{:=<72}", "");
    eprintln!("SUMMARY  ({}s, {} samples)", final_s.elapsed_secs, samples.len());
    eprintln!("{:-<72}", "");
    eprintln!("  RSS initial:  {} KiB", initial.rss_kb);
    eprintln!("  RSS final:    {} KiB", final_s.rss_kb);
    eprintln!("  RSS peak:     {} KiB", peak_rss);
    eprintln!("  RSS growth:   {:.0} KiB ({:+.1}%)", rss_growth, growth_pct);
    eprintln!("  CPU avg:      {:.2}%", avg_cpu);
    eprintln!("  CPU peak:     {:.2}%", max_cpu);

    // Trend: simple linear regression on RSS to detect steady growth
    let trend = linear_trend(&samples);
    eprintln!("  RSS trend:    {:+.2} KiB/min", trend * 60.0);

    eprintln!("{:=<72}", "");

    if growth_pct > leak_threshold as f64 {
        eprintln!(
            "WARNING: RSS grew {:.1}% (threshold {}%). Possible memory leak!",
            growth_pct, leak_threshold
        );
        std::process::exit(1);
    } else {
        eprintln!("OK: RSS growth within threshold.");
    }
}

struct Sample {
    elapsed_secs: u64,
    rss_kb: u64,
    cpu_pct: f64,
}

fn resolve_pid() -> u32 {
    // Check CLI argument first
    if let Some(arg) = std::env::args().nth(1) {
        return arg.parse().expect("Invalid PID argument");
    }

    // Auto-detect: find a running wtop process (not ourselves)
    let my_pid = std::process::id();
    let mut found: Option<u32> = None;

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Ok(pid) = name_str.parse::<u32>() {
                if pid == my_pid {
                    continue;
                }
                let comm_path = format!("/proc/{}/comm", pid);
                if let Ok(comm) = fs::read_to_string(&comm_path) {
                    if comm.trim() == "wtop" {
                        if found.is_some() {
                            eprintln!("Multiple wtop processes found. Specify PID: cargo run --example resource_monitor -- <PID>");
                            std::process::exit(1);
                        }
                        found = Some(pid);
                    }
                }
            }
        }
    }

    match found {
        Some(pid) => pid,
        None => {
            eprintln!("No running wtop process found.");
            eprintln!("Start wtop first, then run this monitor, or supply a PID.");
            std::process::exit(1);
        }
    }
}

fn env_or(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn page_size_kb() -> u64 {
    // Read page size from sysconf
    let output = std::process::Command::new("getconf")
        .arg("PAGESIZE")
        .output();
    match output {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.trim().parse::<u64>().unwrap_or(4096) / 1024
        }
        Err(_) => 4, // assume 4 KiB
    }
}

/// Read RSS from /proc/<pid>/statm (field 1, in pages).
fn read_rss(pid: u32, page_size_kb: u64) -> u64 {
    let path = format!("/proc/{}/statm", pid);
    if let Ok(content) = fs::read_to_string(&path) {
        let fields: Vec<&str> = content.split_whitespace().collect();
        if fields.len() >= 2 {
            if let Ok(pages) = fields[1].parse::<u64>() {
                return pages * page_size_kb;
            }
        }
    }
    0
}

/// Read CPU usage as a percentage since last call.
fn read_cpu(pid: u32, prev: &mut Option<(u64, u64)>) -> f64 {
    let proc_jiffies = read_proc_jiffies(pid);
    let sys_jiffies = read_system_jiffies();

    let pct = if let (Some((prev_proc, prev_sys)), Some(pj), Some(sj)) =
        (*prev, proc_jiffies, sys_jiffies)
    {
        let dp = pj.saturating_sub(prev_proc) as f64;
        let ds = sj.saturating_sub(prev_sys) as f64;
        if ds > 0.0 { (dp / ds) * 100.0 } else { 0.0 }
    } else {
        0.0
    };

    if let (Some(pj), Some(sj)) = (proc_jiffies, sys_jiffies) {
        *prev = Some((pj, sj));
    }

    pct
}

/// Read utime + stime from /proc/<pid>/stat (fields 13 + 14, 0-indexed).
fn read_proc_jiffies(pid: u32) -> Option<u64> {
    let path = format!("/proc/{}/stat", pid);
    let content = fs::read_to_string(&path).ok()?;
    // Fields after the comm (which is in parens) — skip past the closing ')'
    let after_comm = content.rsplit_once(')')?.1;
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    // fields[0] = state, [1] = ppid, ... [11] = utime, [12] = stime
    if fields.len() > 12 {
        let utime: u64 = fields[11].parse().ok()?;
        let stime: u64 = fields[12].parse().ok()?;
        Some(utime + stime)
    } else {
        None
    }
}

/// Read total system jiffies from /proc/stat (first "cpu" line, sum of all fields).
fn read_system_jiffies() -> Option<u64> {
    let file = fs::File::open("/proc/stat").ok()?;
    let reader = io::BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        if line.starts_with("cpu ") {
            let total: u64 = line
                .split_whitespace()
                .skip(1)
                .filter_map(|f| f.parse::<u64>().ok())
                .sum();
            return Some(total);
        }
    }
    None
}

/// Simple linear regression: returns slope of RSS (KiB/sec).
fn linear_trend(samples: &[Sample]) -> f64 {
    let n = samples.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let sum_x: f64 = samples.iter().map(|s| s.elapsed_secs as f64).sum();
    let sum_y: f64 = samples.iter().map(|s| s.rss_kb as f64).sum();
    let sum_xy: f64 = samples.iter().map(|s| s.elapsed_secs as f64 * s.rss_kb as f64).sum();
    let sum_xx: f64 = samples.iter().map(|s| (s.elapsed_secs as f64).powi(2)).sum();

    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < f64::EPSILON {
        return 0.0;
    }
    (n * sum_xy - sum_x * sum_y) / denom
}
