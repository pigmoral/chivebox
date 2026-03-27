use std::fs;
use std::path::Path;

pub fn main(args: &[String]) -> i32 {
    if args.is_empty() {
        return do_show();
    }

    match args[0].as_str() {
        "show" | "list" => do_show(),
        "flush" => do_flush(args),
        "help" | "-h" | "--help" => {
            print_help();
            0
        }
        _ => {
            eprintln!("ip neigh: '{}' is not a valid command", args[0]);
            1
        }
    }
}

fn print_help() {
    println!("Usage: ip neigh COMMAND");
    println!();
    println!("Commands:");
    println!("  ip neigh show                 - show neighbor entries");
    println!("  ip neigh flush [DEV]          - flush neighbor entries");
    println!();
    println!("Help:");
    println!("  ip neigh help                 - display this help message");
}

fn do_show() -> i32 {
    match fs::read_to_string("/proc/net/arp") {
        Ok(text) => {
            for (idx, line) in text.lines().enumerate() {
                if idx == 0 {
                    continue;
                }
                println!("{}", line);
            }
            0
        }
        Err(e) => {
            eprintln!("ip neigh: {}", e);
            1
        }
    }
}

fn do_flush(args: &[String]) -> i32 {
    if args.len() > 2 {
        eprintln!("ip neigh flush: usage: ip neigh flush [DEV]");
        return 1;
    }
    if let Some(dev) = args.get(1) {
        let path = Path::new("/proc/sys/net/ipv4/neigh")
            .join(dev)
            .join("gc_stale_time");
        if !path.exists() {
            eprintln!("ip neigh flush: device '{}' not found", dev);
            return 1;
        }
    }
    eprintln!("ip neigh flush: not supported in this build yet");
    1
}
