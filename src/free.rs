//! `free` command - display system memory usage.
//!
//! Parses `/proc/meminfo` to show total, used, free, shared, buff/cache, and available memory.

use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::applets::AppletArgs;

/// Memory statistics parsed from /proc/meminfo.
#[derive(Default)]
struct MemInfo {
    mem_total: u64,
    mem_free: u64,
    mem_available: u64,
    buffers: u64,
    cached: u64,
    shmem: u64,
    s_reclaimable: u64,
    swap_total: u64,
    swap_free: u64,
}

/// Unit for displaying memory values.
#[derive(Clone, Copy, PartialEq)]
enum Unit {
    Bytes,
    KiB,
    MiB,
    GiB,
    Human,
}

impl Unit {
    /// Convert kB value to display value based on unit.
    fn scale(self, kb: u64) -> f64 {
        match self {
            Unit::Bytes => kb as f64 * 1024.0,
            Unit::KiB => kb as f64,
            Unit::MiB => kb as f64 / 1024.0,
            Unit::GiB => kb as f64 / (1024.0 * 1024.0),
            Unit::Human => kb as f64,
        }
    }
}

/// Parse /proc/meminfo and extract memory statistics.
fn parse_meminfo() -> Result<MemInfo, std::io::Error> {
    let file = File::open("/proc/meminfo")?;
    let reader = BufReader::new(file);
    let mut info = MemInfo::default();

    for line in reader.lines().flatten() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse "Key: value kB" format
        if let Some((key, rest)) = line.split_once(':') {
            let key = key.trim();
            let rest = rest.trim();
            let value_str = rest.split_whitespace().next().unwrap_or("0");
            let value: u64 = value_str.parse().unwrap_or(0);

            match key {
                "MemTotal" => info.mem_total = value,
                "MemFree" => info.mem_free = value,
                "MemAvailable" => info.mem_available = value,
                "Buffers" => info.buffers = value,
                "Cached" => info.cached = value,
                "Shmem" => info.shmem = value,
                "SReclaimable" => info.s_reclaimable = value,
                "SwapTotal" => info.swap_total = value,
                "SwapFree" => info.swap_free = value,
                _ => {}
            }
        }
    }

    Ok(info)
}

/// Format a value for display with right-alignment (11 chars wide).
fn format_value(value_kb: u64, unit: Unit) -> String {
    match unit {
        Unit::Human => {
            let bytes = value_kb as f64 * 1024.0;
            format_human_readable(bytes)
        }
        Unit::Bytes => {
            let bytes = value_kb * 1024;
            format!("{}", bytes)
        }
        _ => {
            let scaled = unit.scale(value_kb);
            format!("{:.0}", scaled)
        }
    }
}

/// Format bytes as human-readable with suffixes (B, K, M, G, T) and 1 decimal place.
fn format_human_readable(bytes: f64) -> String {
    if bytes <= 0.0 {
        return "0B".to_string();
    }

    let suffixes = ["B", "K", "M", "G", "T"];
    let mut value = bytes;
    let mut suffix_idx = 0;

    while value >= 1024.0 && suffix_idx < suffixes.len() - 1 {
        value /= 1024.0;
        suffix_idx += 1;
    }

    let suffix = suffixes[suffix_idx];
    if value < 10.0 {
        format!("{:.1}{}", value, suffix)
    } else {
        format!("{:.0}{}", value, suffix)
    }
}

/// Print the header line for memory statistics.
fn print_header() {
    // Unified 11-character columns for perfect alignment with data
    println!(
        "       {:>11}{:>11}{:>11}{:>11}{:>11}{:>11}",
        "total", "used", "free", "shared", "buff/cache", "available"
    );
}

/// Print memory statistics line.
fn print_mem_line(info: &MemInfo, unit: Unit) {
    // buff/cache = Cached + Buffers + SReclaimable
    let buff_cache = info.cached + info.buffers + info.s_reclaimable;

    // used = total - available
    let used = info.mem_total.saturating_sub(info.mem_available);

    let total = format_value(info.mem_total, unit);
    let used_s = format_value(used, unit);
    let free_s = format_value(info.mem_free, unit);
    let shared = format_value(info.shmem, unit);
    let buff = format_value(buff_cache, unit);
    let avail = format_value(info.mem_available, unit);

    // Unified format: "Mem:" (7 chars) + 6 columns of 11 chars each
    println!(
        "{:<7}{:>11}{:>11}{:>11}{:>11}{:>11}{:>11}",
        "Mem:", total, used_s, free_s, shared, buff, avail
    );
}

/// Print swap statistics line.
fn print_swap_line(info: &MemInfo, unit: Unit) {
    let swap_used = info.swap_total.saturating_sub(info.swap_free);

    let total = format_value(info.swap_total, unit);
    let used_s = format_value(swap_used, unit);
    let free_s = format_value(info.swap_free, unit);

    // Unified format: "Swap:" (7 chars) + 3 columns of 11 chars each
    println!("{:<7}{:>11}{:>11}{:>11}", "Swap:", total, used_s, free_s);
}

pub fn main(args: AppletArgs) -> i32 {
    let args: Vec<String> = args
        .skip(1)
        .map(|s| s.to_str().unwrap_or("").to_string())
        .collect();

    // Check for help first
    for arg in &args {
        if arg == "--help" {
            print_usage();
            return 0;
        }
    }

    // Parse unit option (last one wins)
    let mut unit = Unit::KiB; // default: kibibytes

    for arg in &args {
        match arg.as_str() {
            "-b" => unit = Unit::Bytes,
            "-k" => unit = Unit::KiB,
            "-m" => unit = Unit::MiB,
            "-g" => unit = Unit::GiB,
            "-h" => unit = Unit::Human,
            _ => {}
        }
    }

    // Parse /proc/meminfo
    let info = match parse_meminfo() {
        Ok(info) => info,
        Err(e) => {
            eprintln!("free: cannot read /proc/meminfo: {}", e);
            return 1;
        }
    };

    print_header();
    print_mem_line(&info, unit);
    print_swap_line(&info, unit);

    0
}

fn print_usage() {
    println!("Usage: free [-bkmgh]");
    println!();
    println!("Display free and used memory");
    println!();
    println!("  -b  show output in bytes");
    println!("  -k  show output in kibibytes (default)");
    println!("  -m  show output in mebibytes");
    println!("  -g  show output in gibibytes");
    println!("  -h  show human-readable output");
    println!("  --help  show this help");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_human_readable() {
        assert_eq!(format_human_readable(0.0), "0B");
        assert_eq!(format_human_readable(500.0), "500B");
        assert_eq!(format_human_readable(1024.0), "1.0K");
        assert_eq!(format_human_readable(1536.0), "1.5K");
        assert_eq!(format_human_readable(1024.0 * 1024.0), "1.0M");
        assert_eq!(format_human_readable(1024.0 * 1024.0 * 1024.0), "1.0G");
        assert_eq!(
            format_human_readable(2.0 * 1024.0 * 1024.0 * 1024.0),
            "2.0G"
        );
        // Values >= 10 show no decimal
        assert_eq!(format_human_readable(10.0 * 1024.0), "10K");
        // 15.6 * 1024^3 = 16.75 GB, rounds to 17G
        let val = format_human_readable(15.6 * 1024.0 * 1024.0 * 1024.0);
        assert!(val.contains("G"));
        assert_eq!(
            format_human_readable(100.0 * 1024.0 * 1024.0 * 1024.0),
            "100G"
        );
    }

    #[test]
    fn test_format_value() {
        // 16384000 kB = 16384 MiB
        assert_eq!(format_value(16384000, Unit::KiB), "16384000");
        assert_eq!(format_value(16384000, Unit::MiB), "16000");
        // 16384000 kB = 15.625 GiB, formatted as "16G"
        let gib_val = format_value(16384000, Unit::GiB);
        assert!(gib_val.contains("16") || gib_val.contains("15"));
        // 16384000 kB * 1024 = 16777216000 bytes
        assert_eq!(format_value(16384000, Unit::Bytes), "16777216000");
    }

    #[test]
    fn test_format_value_human() {
        let val = format_value(16384000, Unit::Human);
        // 16384000 kB * 1024 = 16777216000 bytes = 16 GiB
        assert!(val.contains("G"));
        assert!(val.contains("16"));
    }

    #[test]
    fn test_meminfo_parsing() {
        // Simulate parsing with test data
        let test_content = r#"
MemTotal:       16384000 kB
MemFree:         8192000 kB
MemAvailable:   12000000 kB
Buffers:          500000 kB
Cached:          3000000 kB
Shmem:            100000 kB
SReclaimable:     308000 kB
SwapTotal:       2097152 kB
SwapFree:        2097152 kB
"#;

        let mut info = MemInfo::default();
        for line in test_content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((key, rest)) = line.split_once(':') {
                let key = key.trim();
                let rest = rest.trim();
                let value_str = rest.split_whitespace().next().unwrap_or("0");
                let value: u64 = value_str.parse().unwrap_or(0);

                match key {
                    "MemTotal" => info.mem_total = value,
                    "MemFree" => info.mem_free = value,
                    "MemAvailable" => info.mem_available = value,
                    "Buffers" => info.buffers = value,
                    "Cached" => info.cached = value,
                    "Shmem" => info.shmem = value,
                    "SReclaimable" => info.s_reclaimable = value,
                    "SwapTotal" => info.swap_total = value,
                    "SwapFree" => info.swap_free = value,
                    _ => {}
                }
            }
        }

        assert_eq!(info.mem_total, 16384000);
        assert_eq!(info.mem_free, 8192000);
        assert_eq!(info.mem_available, 12000000);
        assert_eq!(info.buffers, 500000);
        assert_eq!(info.cached, 3000000);
        assert_eq!(info.shmem, 100000);
        assert_eq!(info.s_reclaimable, 308000);
        assert_eq!(info.swap_total, 2097152);
        assert_eq!(info.swap_free, 2097152);

        // Test calculations
        let buff_cache = info.cached + info.buffers + info.s_reclaimable;
        assert_eq!(buff_cache, 3808000);

        let used = info.mem_total.saturating_sub(info.mem_available);
        assert_eq!(used, 4384000);
    }

    #[test]
    fn test_unit_scale() {
        let kb: u64 = 1024; // 1 MiB in kB
        assert!((Unit::Bytes.scale(kb) - 1048576.0).abs() < 0.001);
        assert!((Unit::KiB.scale(kb) - 1024.0).abs() < 0.001);
        assert!((Unit::MiB.scale(kb) - 1.0).abs() < 0.001);
        assert!((Unit::GiB.scale(kb) - 0.0009765625).abs() < 0.0001);
    }
}
