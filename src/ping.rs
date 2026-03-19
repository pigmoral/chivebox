use crate::applets::AppletArgs;
use std::net::{Ipv4Addr, ToSocketAddrs};
use std::time::{Duration, Instant};

const ICMP_ECHO_REQUEST: u8 = 8;
const ICMP_ECHO_REPLY: u8 = 0;
const DEFAULT_DATA_SIZE: usize = 56;
const DEFAULT_TIMEOUT_SECS: u64 = 10;
const ICMP_HEADER_LEN: usize = 8;

pub fn main(args: AppletArgs) -> i32 {
    let args: Vec<String> = args
        .skip(1)
        .map(|s| s.to_str().unwrap_or("").to_string())
        .collect();

    if args.is_empty() {
        print_help();
        return 0;
    }

    let mut count: Option<u32> = None;
    let mut interval = Duration::from_secs(1);
    let mut size = DEFAULT_DATA_SIZE;
    let mut timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
    let mut deadline: Option<Instant> = None;
    let mut ttl: u8 = 64;
    let mut quiet = false;
    let mut numeric = false;
    let mut host: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-c" => {
                count = Some(
                    match parse_u32_arg(&args, &mut i, "ping: -c requires an argument") {
                        Ok(v) => v,
                        Err(code) => return code,
                    },
                );
            }
            "-i" => {
                let secs = match parse_f64_arg(&args, &mut i, "ping: -i requires an argument") {
                    Ok(v) => v,
                    Err(code) => return code,
                };
                interval = Duration::from_secs_f64(secs.max(0.0));
            }
            "-s" => {
                size = match parse_usize_arg(&args, &mut i, "ping: -s requires an argument") {
                    Ok(v) => v,
                    Err(code) => return code,
                };
            }
            "-W" => {
                let secs = match parse_u64_arg(&args, &mut i, "ping: -W requires an argument") {
                    Ok(v) => v,
                    Err(code) => return code,
                };
                timeout = Duration::from_secs(secs);
            }
            "-w" => {
                let secs = match parse_u64_arg(&args, &mut i, "ping: -w requires an argument") {
                    Ok(v) => v,
                    Err(code) => return code,
                };
                deadline = Some(Instant::now() + Duration::from_secs(secs));
            }
            "-t" => {
                ttl = match parse_u8_arg(&args, &mut i, "ping: -t requires an argument") {
                    Ok(v) => v,
                    Err(code) => return code,
                };
            }
            "-n" => {
                numeric = true;
                i += 1;
            }
            "-q" => {
                quiet = true;
                i += 1;
            }
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            opt if opt.starts_with('-') => {
                eprintln!("ping: unsupported option '{}'", opt);
                return 1;
            }
            value => {
                if host.is_none() {
                    host = Some(value.to_string());
                }
                i += 1;
            }
        }
    }

    let host = match host {
        Some(h) => h,
        None => {
            eprintln!("ping: missing host operand");
            return 1;
        }
    };

    let target = match resolve_host(&host, numeric) {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("ping: {}", e);
            return 1;
        }
    };

    run_ping(target, count, interval, size, timeout, deadline, ttl, quiet)
}

fn print_help() {
    println!("Usage: ping [OPTIONS] HOST");
    println!();
    println!("Send ICMP ECHO_REQUESTs to network hosts.");
    println!();
    println!("Options:");
    println!("  -c COUNT      Stop after sending COUNT packets");
    println!("  -i SECS       Wait SECS between packets");
    println!("  -s SIZE       Payload size in bytes (default 56)");
    println!("  -W SECS       Per-packet reply timeout");
    println!("  -w SECS       Deadline for the whole ping run");
    println!("  -t TTL        Set IPv4 TTL");
    println!("  -n            Numeric output only");
    println!("  -q            Quiet output");
}

fn resolve_host(host: &str, numeric: bool) -> Result<Ipv4Addr, String> {
    if numeric {
        return host
            .parse()
            .map_err(|_| format!("{}: Name or service not known", host));
    }

    if let Ok(ip) = host.parse() {
        return Ok(ip);
    }

    (host, 0)
        .to_socket_addrs()
        .map_err(|e| format!("{}: {}", host, e))?
        .find_map(|addr| match addr.ip() {
            std::net::IpAddr::V4(v4) => Some(v4),
            _ => None,
        })
        .ok_or_else(|| format!("{}: Name or service not known", host))
}

fn run_ping(
    target: Ipv4Addr,
    count: Option<u32>,
    interval: Duration,
    size: usize,
    timeout: Duration,
    deadline: Option<Instant>,
    ttl: u8,
    quiet: bool,
) -> i32 {
    let pid = std::process::id() as u16;
    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_RAW, libc::IPPROTO_ICMP) };

    if sock < 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EPERM as i32) {
            eprintln!("ping: permission denied, are you root?");
        } else {
            eprintln!("ping: cannot create raw socket");
        }
        return 1;
    }

    let addr = libc::sockaddr_in {
        sin_family: libc::AF_INET as u16,
        sin_port: 0,
        sin_addr: libc::in_addr {
            s_addr: u32::from(target).to_be(),
        },
        sin_zero: [0; 8],
    };

    if unsafe {
        libc::connect(
            sock,
            &addr as *const libc::sockaddr_in as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        )
    } < 0
    {
        eprintln!("ping: cannot connect to target");
        unsafe { libc::close(sock) };
        return 1;
    }

    unsafe {
        libc::setsockopt(
            sock,
            libc::IPPROTO_IP,
            libc::IP_TTL,
            &ttl as *const u8 as *const libc::c_void,
            std::mem::size_of::<u8>() as libc::socklen_t,
        );
    }

    if !quiet {
        println!("PING {}: {} data bytes", target, size);
    }

    let mut transmitted = 0u32;
    let mut received = 0u32;
    let mut min_rtt = f64::INFINITY;
    let mut max_rtt = 0.0f64;
    let mut sum_rtt = 0.0f64;
    let start_time = Instant::now();

    loop {
        if let Some(limit) = count {
            if transmitted >= limit {
                break;
            }
        }
        if let Some(end) = deadline {
            if Instant::now() >= end {
                break;
            }
        }

        let seq = transmitted as u16;
        let send_time = Instant::now();
        if let Err(e) = send_ping(sock, pid, seq, size) {
            eprintln!("ping: send failed: {}", e);
            unsafe { libc::close(sock) };
            return 1;
        }
        transmitted += 1;

        match recv_ping(sock, pid, seq, timeout, send_time) {
            Ok((reply_ttl, rtt)) => {
                received += 1;
                sum_rtt += rtt;
                min_rtt = min_rtt.min(rtt);
                max_rtt = max_rtt.max(rtt);
                if !quiet {
                    println!(
                        "{} bytes from {}: icmp_seq={} ttl={} time={:.3} ms",
                        size + ICMP_HEADER_LEN,
                        target,
                        seq,
                        reply_ttl,
                        rtt
                    );
                }
            }
            Err(_) => {
                if !quiet {
                    eprintln!("ping: timeout waiting for response");
                }
            }
        }

        if let Some(limit) = count {
            if transmitted >= limit {
                break;
            }
        }

        if count.map_or(true, |limit| transmitted < limit) {
            std::thread::sleep(interval);
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    println!("\n--- {} ping statistics ---", target);
    let loss = if transmitted > 0 {
        ((transmitted - received) as f64 / transmitted as f64 * 100.0).round() as u32
    } else {
        0
    };
    println!(
        "{} packets transmitted, {} packets received, {}% packet loss, time {:.0}ms",
        transmitted,
        received,
        loss,
        elapsed * 1000.0
    );

    if received > 0 {
        println!(
            "rtt min/avg/max = {:.3}/{:.3}/{:.3} ms",
            min_rtt,
            sum_rtt / received as f64,
            max_rtt
        );
    }

    unsafe { libc::close(sock) };
    if received == 0 {
        1
    } else {
        0
    }
}

fn send_ping(sock: i32, id: u16, seq: u16, size: usize) -> Result<(), std::io::Error> {
    let mut packet = vec![0u8; ICMP_HEADER_LEN + size];
    packet[0] = ICMP_ECHO_REQUEST;
    packet[1] = 0;
    packet[2] = 0;
    packet[3] = 0;
    packet[4..6].copy_from_slice(&id.to_be_bytes());
    packet[6..8].copy_from_slice(&seq.to_be_bytes());
    for (i, byte) in packet[ICMP_HEADER_LEN..].iter_mut().enumerate() {
        *byte = (i & 0xff) as u8;
    }

    let cksum = checksum(&packet);
    packet[2..4].copy_from_slice(&cksum.to_be_bytes());

    let ret = unsafe {
        libc::send(
            sock,
            packet.as_ptr() as *const libc::c_void,
            packet.len(),
            0,
        )
    };

    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn recv_ping(
    sock: i32,
    id: u16,
    seq: u16,
    timeout: Duration,
    send_time: Instant,
) -> Result<(u8, f64), std::io::Error> {
    let mut packet = vec![0u8; 2048];
    let tv = libc::timeval {
        tv_sec: timeout.as_secs() as libc::time_t,
        tv_usec: timeout.subsec_micros() as libc::suseconds_t,
    };

    unsafe {
        libc::setsockopt(
            sock,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const libc::timeval as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );
    }

    loop {
        let ret = unsafe {
            libc::recv(
                sock,
                packet.as_mut_ptr() as *mut libc::c_void,
                packet.len(),
                0,
            )
        };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::WouldBlock
                || err.kind() == std::io::ErrorKind::TimedOut
            {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
            }
            return Err(err);
        }

        let ret = ret as usize;
        if ret < 20 {
            continue;
        }

        let ip_header_len = ((packet[0] & 0x0f) as usize) * 4;
        if ret < ip_header_len + ICMP_HEADER_LEN {
            continue;
        }

        let icmp = &packet[ip_header_len..ret];
        let icmp_type = icmp[0];
        let recv_id = u16::from_be_bytes([icmp[4], icmp[5]]);
        let recv_seq = u16::from_be_bytes([icmp[6], icmp[7]]);

        if icmp_type != ICMP_ECHO_REPLY || recv_id != id || recv_seq != seq {
            continue;
        }

        let rtt = send_time.elapsed().as_secs_f64() * 1000.0;
        return Ok((packet[8], rtt));
    }
}

fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);

    for chunk in &mut chunks {
        sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
    }

    if let Some(&byte) = chunks.remainder().first() {
        sum += u16::from_be_bytes([byte, 0]) as u32;
    }

    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }

    !(sum as u16)
}

fn parse_u32_arg(args: &[String], idx: &mut usize, msg: &str) -> Result<u32, i32> {
    if *idx + 1 >= args.len() {
        eprintln!("{}", msg);
        return Err(1);
    }
    let value = args[*idx + 1].parse().unwrap_or(0);
    *idx += 2;
    Ok(value)
}

fn parse_u64_arg(args: &[String], idx: &mut usize, msg: &str) -> Result<u64, i32> {
    if *idx + 1 >= args.len() {
        eprintln!("{}", msg);
        return Err(1);
    }
    let value = args[*idx + 1].parse().unwrap_or(0);
    *idx += 2;
    Ok(value)
}

fn parse_usize_arg(args: &[String], idx: &mut usize, msg: &str) -> Result<usize, i32> {
    if *idx + 1 >= args.len() {
        eprintln!("{}", msg);
        return Err(1);
    }
    let value = args[*idx + 1].parse().unwrap_or(DEFAULT_DATA_SIZE);
    *idx += 2;
    Ok(value)
}

fn parse_u8_arg(args: &[String], idx: &mut usize, msg: &str) -> Result<u8, i32> {
    if *idx + 1 >= args.len() {
        eprintln!("{}", msg);
        return Err(1);
    }
    let value = args[*idx + 1].parse().unwrap_or(64);
    *idx += 2;
    Ok(value)
}

fn parse_f64_arg(args: &[String], idx: &mut usize, msg: &str) -> Result<f64, i32> {
    if *idx + 1 >= args.len() {
        eprintln!("{}", msg);
        return Err(1);
    }
    let value = args[*idx + 1].parse().unwrap_or(1.0);
    *idx += 2;
    Ok(value)
}
