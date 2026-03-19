use std::fs;
use std::mem::size_of;
use std::path::Path;
use std::str::FromStr;

use super::netlink::*;

pub fn main(args: &[String]) -> i32 {
    if args.is_empty() {
        return do_show(None, false);
    }

    match args[0].as_str() {
        "show" | "list" | "a" => {
            let iface = args
                .iter()
                .skip(1)
                .find(|s| !s.starts_with('-'))
                .map(|s| s.as_str());
            do_show(iface, false)
        }
        "add" => do_add(args),
        "del" | "delete" => do_del(args),
        "help" | "-h" | "--help" => {
            print_help();
            0
        }
        _ => {
            eprintln!("ip addr: '{}' is not a valid command", args[0]);
            1
        }
    }
}

fn print_help() {
    println!("Usage: ip addr COMMAND");
    println!();
    println!("Commands:");
    println!("  ip addr show [DEVICE]              show addresses");
    println!("  ip addr add ADDR dev DEVICE        add IPv4 address");
    println!("  ip addr del ADDR dev DEVICE        delete IPv4 address");
    println!("Aliases:");
    println!("  ip a show, ip a add, ip a del");
}

fn do_show(iface: Option<&str>, _one_line: bool) -> i32 {
    match addr_show(iface) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("ip addr: {}", e);
            1
        }
    }
}

fn do_add(args: &[String]) -> i32 {
    let mut idx = 1;
    let addr = match args.get(idx) {
        Some(v) => v.clone(),
        None => {
            eprintln!("ip addr add: usage: ip addr add ADDR dev DEVICE");
            return 1;
        }
    };
    idx += 1;

    let mut dev = None;
    while idx < args.len() {
        match args[idx].as_str() {
            "dev" => {
                idx += 1;
                dev = args.get(idx).cloned();
                idx += 1;
            }
            "local" | "label" | "scope" | "broadcast" | "peer" => {
                idx += 2;
            }
            _ => idx += 1,
        }
    }

    let dev = match dev {
        Some(v) => v,
        None => {
            eprintln!("ip addr add: missing dev DEVICE");
            return 1;
        }
    };

    match addr_add(&dev, &addr) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("ip addr add: {}", e);
            1
        }
    }
}

fn do_del(args: &[String]) -> i32 {
    let mut idx = 1;
    let addr = match args.get(idx) {
        Some(v) => v.clone(),
        None => {
            eprintln!("ip addr del: usage: ip addr del ADDR dev DEVICE");
            return 1;
        }
    };
    idx += 1;

    let mut dev = None;
    while idx < args.len() {
        match args[idx].as_str() {
            "dev" => {
                idx += 1;
                dev = args.get(idx).cloned();
                idx += 1;
            }
            _ => idx += 1,
        }
    }

    let dev = match dev {
        Some(v) => v,
        None => {
            eprintln!("ip addr del: missing dev DEVICE");
            return 1;
        }
    };

    match addr_del(&dev, &addr) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("ip addr del: {}", e);
            1
        }
    }
}

fn addr_show(iface: Option<&str>) -> Result<(), String> {
    let net_dir = Path::new("/sys/class/net");
    let mut interfaces: Vec<(u32, String)> = if let Some(name) = iface {
        vec![(get_ifindex(name), name.to_string())]
    } else {
        fs::read_dir(net_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                Some((get_ifindex(&name), name))
            })
            .collect()
    };
    interfaces.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    for (ifindex, name) in interfaces {
        println!("{}: {}", ifindex, name);
        let flags = read_flags(&name);
        let mtu = read_string(net_dir.join(&name).join("mtu")).unwrap_or_else(|_| "0".into());
        let state = if flags & IFF_UP != 0 { "UP" } else { "DOWN" };
        println!(
            "    {} mtu {} state {}",
            format_flags(flags),
            mtu.trim(),
            state
        );

        let mac = read_string(net_dir.join(&name).join("address")).unwrap_or_default();
        let brd = read_string(net_dir.join(&name).join("broadcast")).unwrap_or_default();
        if !mac.trim().is_empty() && mac.trim() != "00:00:00:00:00:00" {
            println!(
                "    link/ether {} brd {}",
                mac.trim(),
                if brd.trim().is_empty() {
                    "ff:ff:ff:ff:ff:ff"
                } else {
                    brd.trim()
                }
            );
        }

        for addr in collect_ipv4_addrs(&name)? {
            println!("    inet {} scope {}", addr.addr, addr.scope);
        }
        for addr in collect_ipv6_addrs(&name)? {
            println!("    inet6 {} scope {}", addr.addr, addr.scope);
        }
    }

    Ok(())
}

struct V4AddrLine {
    addr: String,
    scope: String,
}

fn collect_ipv4_addrs(name: &str) -> Result<Vec<V4AddrLine>, String> {
    let mut sock = NetlinkSocket::connect().map_err(|e| e.to_string())?;
    let mut payload = Vec::new();
    let msg = IfAddrMsg {
        ifa_family: AF_INET_U8,
        ifa_prefixlen: 0,
        ifa_flags: 0,
        ifa_scope: 0,
        ifa_index: 0,
    };
    put_struct(&mut payload, &msg);
    send_netlink(
        sock.fd(),
        &payload,
        RTM_GETADDR,
        NLM_F_DUMP | NLM_F_REQUEST,
        sock.next_seq(),
    )
    .map_err(|e| e.to_string())?;
    let buf = recv_messages(sock.fd()).map_err(|e| e.to_string())?;
    let mut result = Vec::new();

    for (hdr, msg) in parse_nlmsgs(&buf) {
        if hdr.nlmsg_type == 3 || msg.len() < size_of::<IfAddrMsg>() {
            continue;
        }
        let ifa = unsafe { &*(msg.as_ptr() as *const IfAddrMsg) };
        if ifa.ifa_family != AF_INET_U8 {
            continue;
        }
        let attrs = parse_attrs(&msg[size_of::<IfAddrMsg>()..]);
        let mut addr = None;
        let mut label = None;
        for (typ, val) in attrs {
            match typ {
                IFA_ADDRESS | IFA_LOCAL => addr = parse_ipv4(&val).map(|v| v.to_string()),
                IFA_LABEL => label = Some(read_cstring(&val)),
                _ => {}
            }
        }
        if let Some(addr) = addr {
            if label.as_deref() == Some(name) || label.is_none() {
                result.push(V4AddrLine {
                    addr,
                    scope: scope_name(ifa.ifa_scope).into(),
                });
            }
        }
    }
    Ok(result)
}

struct V6AddrLine {
    addr: String,
    scope: String,
}

fn collect_ipv6_addrs(name: &str) -> Result<Vec<V6AddrLine>, String> {
    let mut result = Vec::new();
    let content = fs::read_to_string("/proc/net/if_inet6").map_err(|e| e.to_string())?;
    let ifindex = get_ifindex(name);
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let idx = u32::from_str_radix(parts[1], 16).unwrap_or(0);
        if idx != ifindex {
            continue;
        }
        result.push(V6AddrLine {
            addr: format_ipv6(parts[0]),
            scope: scope_name(u8::from_str_radix(parts[3], 16).unwrap_or(0)).into(),
        });
    }
    Ok(result)
}

fn addr_add(dev: &str, addr: &str) -> Result<(), String> {
    let (ip, prefix) = parse_addr_with_prefix(addr)?;
    let ifindex = get_ifindex(dev);
    if ifindex == 0 {
        return Err(format!("device '{}' not found", dev));
    }
    let mut payload = Vec::new();
    put_struct(
        &mut payload,
        &IfAddrMsg {
            ifa_family: AF_INET_U8,
            ifa_prefixlen: prefix,
            ifa_flags: 0,
            ifa_scope: RT_SCOPE_UNIVERSE,
            ifa_index: ifindex,
        },
    );
    put_attr(&mut payload, IFA_LOCAL, &ip.octets());
    put_attr(&mut payload, IFA_ADDRESS, &ip.octets());
    put_attr(&mut payload, IFA_LABEL, dev.as_bytes());
    let mut sock = NetlinkSocket::connect().map_err(|e| e.to_string())?;
    send_netlink(
        sock.fd(),
        &payload,
        RTM_NEWADDR,
        NLM_F_REQUEST | NLM_F_ACK | NLM_F_CREATE | NLM_F_EXCL,
        sock.next_seq(),
    )
    .map_err(|e| e.to_string())
}

fn addr_del(dev: &str, addr: &str) -> Result<(), String> {
    let (ip, prefix) = parse_addr_with_prefix(addr)?;
    let ifindex = get_ifindex(dev);
    if ifindex == 0 {
        return Err(format!("device '{}' not found", dev));
    }
    let mut payload = Vec::new();
    put_struct(
        &mut payload,
        &IfAddrMsg {
            ifa_family: AF_INET_U8,
            ifa_prefixlen: prefix,
            ifa_flags: 0,
            ifa_scope: RT_SCOPE_UNIVERSE,
            ifa_index: ifindex,
        },
    );
    put_attr(&mut payload, IFA_LOCAL, &ip.octets());
    put_attr(&mut payload, IFA_ADDRESS, &ip.octets());
    put_attr(&mut payload, IFA_LABEL, dev.as_bytes());
    let mut sock = NetlinkSocket::connect().map_err(|e| e.to_string())?;
    send_netlink(
        sock.fd(),
        &payload,
        RTM_DELADDR,
        NLM_F_REQUEST | NLM_F_ACK,
        sock.next_seq(),
    )
    .map_err(|e| e.to_string())
}

fn parse_addr_with_prefix(addr: &str) -> Result<(std::net::Ipv4Addr, u8), String> {
    if let Some((ip, prefix)) = addr.split_once('/') {
        let ip = std::net::Ipv4Addr::from_str(ip).map_err(|e| e.to_string())?;
        let prefix = prefix.parse::<u8>().map_err(|e| e.to_string())?;
        Ok((ip, prefix))
    } else {
        Ok((
            std::net::Ipv4Addr::from_str(addr).map_err(|e| e.to_string())?,
            32,
        ))
    }
}

fn read_string(path: impl AsRef<Path>) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| e.to_string())
}

fn read_flags(name: &str) -> u32 {
    let path = Path::new("/sys/class/net").join(name).join("flags");
    read_string(path)
        .ok()
        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x").trim(), 16).ok())
        .unwrap_or(0)
}

fn get_ifindex(name: &str) -> u32 {
    read_string(Path::new("/sys/class/net").join(name).join("ifindex"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn format_flags(flags: u32) -> String {
    let mut out = Vec::new();
    if flags & IFF_UP != 0 {
        out.push("UP");
    }
    if flags & 0x40 != 0 {
        out.push("LOWER_UP");
    }
    if flags & 0x8 != 0 {
        out.push("RUNNING");
    }
    if flags & 0x10 != 0 {
        out.push("MULTICAST");
    }
    if flags & 0x100 != 0 {
        out.push("BROADCAST");
    }
    if flags & 0x200 != 0 {
        out.push("POINTOPOINT");
    }
    if flags & 0x1000 != 0 {
        out.push("LOOPBACK");
    }
    out.join(",")
}

fn scope_name(scope: u8) -> &'static str {
    match scope {
        RT_SCOPE_HOST => "host",
        RT_SCOPE_LINK => "link",
        RT_SCOPE_NOWHERE => "nowhere",
        _ => "global",
    }
}

fn format_ipv6(hex: &str) -> String {
    let mut bytes = [0u8; 16];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate().take(16) {
        bytes[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap_or("00"), 16).unwrap_or(0);
    }
    (0..8)
        .map(|i| format!("{:x}", u16::from_be_bytes([bytes[i * 2], bytes[i * 2 + 1]])))
        .collect::<Vec<_>>()
        .join(":")
}
