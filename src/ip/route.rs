use std::fs;
use std::net::Ipv4Addr;
use std::path::Path;
use std::str::FromStr;

use super::netlink::*;

pub fn main(args: &[String]) -> i32 {
    if args.is_empty() {
        return do_show();
    }

    match args[0].as_str() {
        "show" | "list" => do_show(),
        "get" => do_get(args),
        "add" => do_add(args),
        "del" | "delete" => do_del(args),
        "help" | "-h" | "--help" => {
            print_help();
            0
        }
        _ => {
            eprintln!("ip route: '{}' is not a valid command", args[0]);
            1
        }
    }
}

fn print_help() {
    println!("Usage: ip route COMMAND");
    println!();
    println!("Commands:");
    println!("  ip route show                 - show routing table");
    println!("  ip route get DEST             - get route for destination");
    println!("  ip route add DEST via GW dev DEV - add route");
    println!("  ip route del DEST             - delete route");
    println!();
    println!("Help:");
    println!("  ip route help                 - display this help message");
}

fn do_show() -> i32 {
    match route_show() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("ip route: {}", e);
            1
        }
    }
}

fn do_get(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("ip route get: usage: ip route get DEST");
        return 1;
    }
    let dest = args[1].clone();
    match route_get(&dest) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("ip route get: {}", e);
            1
        }
    }
}

fn do_add(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("ip route add: usage: ip route add DEST [via GW] [dev IFACE]");
        return 1;
    }

    let mut dest = None;
    let mut via = None;
    let mut dev = None;
    let mut src = None;
    let mut metric = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "via" => {
                via = args.get(i + 1).cloned();
                i += 2;
            }
            "dev" => {
                dev = args.get(i + 1).cloned();
                i += 2;
            }
            "src" => {
                src = args.get(i + 1).cloned();
                i += 2;
            }
            "metric" => {
                metric = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            s if dest.is_none() => {
                dest = Some(s.to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let dest = match dest {
        Some(v) => v,
        None => {
            eprintln!("ip route add: missing destination");
            return 1;
        }
    };
    match route_add(
        &dest,
        via.as_deref(),
        dev.as_deref(),
        src.as_deref(),
        metric,
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("ip route add: {}", e);
            1
        }
    }
}

fn do_del(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("ip route del: usage: ip route del DEST");
        return 1;
    }
    let dest = args[1].clone();
    match route_del(&dest) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("ip route del: {}", e);
            1
        }
    }
}

fn route_show() -> Result<(), String> {
    let routes = fs::read_to_string("/proc/net/route").map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for (idx, line) in routes.lines().enumerate() {
        if idx == 0 {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 11 {
            continue;
        }
        let iface = parts[0];
        let dest = parse_hex_ipv4(parts[1]);
        let gateway = parse_hex_ipv4(parts[2]);
        let flags = u32::from_str_radix(parts[3], 16).unwrap_or(0);
        let metric = parts[6];
        let mask = parse_hex_ipv4(parts[7]);
        if flags & 0x1 != 0 {
            rows.push((
                get_ifindex(iface),
                route_line(dest, gateway, iface, mask, metric),
            ));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (_, line) in rows {
        println!("{}", line);
    }
    Ok(())
}

fn route_get(dest: &str) -> Result<(), String> {
    let mut sock = NetlinkSocket::connect().map_err(|e| e.to_string())?;
    let target = Ipv4Addr::from_str(dest).map_err(|e| e.to_string())?;
    let mut payload = Vec::new();
    put_struct(
        &mut payload,
        &RtMsg {
            rtm_family: AF_INET_U8,
            rtm_dst_len: 32,
            rtm_src_len: 0,
            rtm_tos: 0,
            rtm_table: 0,
            rtm_protocol: 0,
            rtm_scope: 0,
            rtm_type: 0,
            rtm_flags: 0,
        },
    );
    put_attr(&mut payload, RTA_DST, &target.octets());
    send_netlink(
        sock.fd(),
        &payload,
        RTM_GETROUTE,
        NLM_F_REQUEST,
        sock.next_seq(),
    )
    .map_err(|e| e.to_string())?;
    let buf = recv_messages(sock.fd()).map_err(|e| e.to_string())?;
    let mut best: Option<(u32, String)> = None;
    for (hdr, msg) in parse_nlmsgs(&buf) {
        if hdr.nlmsg_type == 3 || msg.len() < std::mem::size_of::<RtMsg>() {
            continue;
        }
        let attrs = parse_attrs(&msg[std::mem::size_of::<RtMsg>()..]);
        let mut oif = None;
        let mut gateway = None;
        for (typ, val) in attrs {
            match typ {
                RTA_OIF => oif = read_u32_ne(&val),
                RTA_GATEWAY => gateway = parse_ipv4(&val),
                _ => {}
            }
        }
        let line = format!(
            "{} via {} dev {}",
            dest,
            gateway.unwrap_or(target),
            oif.map_or_else(
                || "unknown".into(),
                |i| ifname(i).unwrap_or_else(|| i.to_string())
            )
        );
        let key = oif.unwrap_or(0);
        if best
            .as_ref()
            .map(|(best_key, _)| key < *best_key)
            .unwrap_or(true)
        {
            best = Some((key, line));
        }
    }
    if let Some((_, line)) = best {
        println!("{}", line);
        Ok(())
    } else {
        Err("no route found".into())
    }
}

fn route_add(
    dest: &str,
    via: Option<&str>,
    dev: Option<&str>,
    src: Option<&str>,
    metric: Option<u32>,
) -> Result<(), String> {
    let target = parse_prefix(dest)?;
    let ifindex = match dev {
        Some(d) => get_ifindex(d),
        None => 0,
    };
    if dev.is_some() && ifindex == 0 {
        return Err(format!("device '{}' not found", dev.unwrap()));
    }
    let mut payload = Vec::new();
    put_struct(
        &mut payload,
        &RtMsg {
            rtm_family: AF_INET_U8,
            rtm_dst_len: target.1,
            rtm_src_len: 0,
            rtm_tos: 0,
            rtm_table: RT_TABLE_MAIN,
            rtm_protocol: RTPROT_BOOT,
            rtm_scope: if via.is_some() {
                RT_SCOPE_UNIVERSE
            } else {
                RT_SCOPE_LINK
            },
            rtm_type: RTN_UNICAST,
            rtm_flags: 0,
        },
    );
    put_attr(&mut payload, RTA_DST, &target.0.octets());
    if let Some(via) = via {
        put_attr(
            &mut payload,
            RTA_GATEWAY,
            &Ipv4Addr::from_str(via).map_err(|e| e.to_string())?.octets(),
        );
    }
    if ifindex != 0 {
        put_attr(&mut payload, RTA_OIF, &ifindex.to_ne_bytes());
    }
    if let Some(src) = src {
        put_attr(
            &mut payload,
            RTA_PREFSRC,
            &Ipv4Addr::from_str(src).map_err(|e| e.to_string())?.octets(),
        );
    }
    if let Some(metric) = metric {
        put_attr(&mut payload, RTA_PRIORITY, &metric.to_ne_bytes());
    }
    let mut sock = NetlinkSocket::connect().map_err(|e| e.to_string())?;
    send_netlink(
        sock.fd(),
        &payload,
        RTM_NEWROUTE,
        NLM_F_REQUEST | NLM_F_ACK | NLM_F_CREATE | NLM_F_EXCL,
        sock.next_seq(),
    )
    .map_err(|e| e.to_string())
}

fn route_del(dest: &str) -> Result<(), String> {
    let target = parse_prefix(dest)?;
    let mut payload = Vec::new();
    put_struct(
        &mut payload,
        &RtMsg {
            rtm_family: AF_INET_U8,
            rtm_dst_len: target.1,
            rtm_src_len: 0,
            rtm_tos: 0,
            rtm_table: RT_TABLE_MAIN,
            rtm_protocol: RTPROT_BOOT,
            rtm_scope: RT_SCOPE_UNIVERSE,
            rtm_type: RTN_UNICAST,
            rtm_flags: 0,
        },
    );
    put_attr(&mut payload, RTA_DST, &target.0.octets());
    let mut sock = NetlinkSocket::connect().map_err(|e| e.to_string())?;
    send_netlink(
        sock.fd(),
        &payload,
        RTM_DELROUTE,
        NLM_F_REQUEST | NLM_F_ACK,
        sock.next_seq(),
    )
    .map_err(|e| e.to_string())
}

fn parse_prefix(s: &str) -> Result<(Ipv4Addr, u8), String> {
    if let Some((ip, prefix)) = s.split_once('/') {
        Ok((
            Ipv4Addr::from_str(ip).map_err(|e| e.to_string())?,
            prefix.parse::<u8>().map_err(|e| e.to_string())?,
        ))
    } else {
        Ok((Ipv4Addr::from_str(s).map_err(|e| e.to_string())?, 32))
    }
}

fn parse_hex_ipv4(hex: &str) -> Ipv4Addr {
    let v = u32::from_str_radix(hex, 16).unwrap_or(0);
    Ipv4Addr::new(
        (v & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        ((v >> 16) & 0xff) as u8,
        ((v >> 24) & 0xff) as u8,
    )
}

fn get_ifindex(name: &str) -> u32 {
    fs::read_to_string(Path::new("/sys/class/net").join(name).join("ifindex"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn ifname(idx: u32) -> Option<String> {
    let net = Path::new("/sys/class/net");
    fs::read_dir(net)
        .ok()?
        .filter_map(|e| e.ok())
        .find_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if get_ifindex(&name) == idx {
                Some(name)
            } else {
                None
            }
        })
}

fn route_line(
    dest: Ipv4Addr,
    gateway: Ipv4Addr,
    iface: &str,
    mask: Ipv4Addr,
    metric: &str,
) -> String {
    if gateway == Ipv4Addr::new(0, 0, 0, 0) {
        format!(
            "{} dev {} scope link src {} metric {}",
            dest, iface, mask, metric
        )
    } else {
        format!("{} via {} dev {} metric {}", dest, gateway, iface, metric)
    }
}
