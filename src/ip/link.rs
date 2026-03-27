use super::netlink::{
    IFF_UP, IfInfoMsg, NLM_F_ACK, NLM_F_REQUEST, NetlinkSocket, RTM_NEWLINK, put_struct,
    send_netlink,
};
use std::fs;
use std::path::Path;

pub fn main(args: &[String]) -> i32 {
    if args.is_empty() {
        return do_show(None);
    }

    match args[0].as_str() {
        "show" | "list" => {
            let iface = args.get(1).filter(|s| !s.starts_with('-'));
            do_show(iface.map(|s| s.as_str()))
        }
        "set" => do_set(args),
        "add" => do_add(args),
        "del" | "delete" => do_del(args),
        "-h" | "--help" | "help" => {
            print_help();
            0
        }
        _ => {
            eprintln!("ip link: '{}' is not a valid command", args[0]);
            1
        }
    }
}

fn print_help() {
    println!("Usage: ip link COMMAND");
    println!();
    println!("Commands:");
    println!("  ip link show [DEVICE]        - show device attributes");
    println!("  ip link set DEVICE up        - bring device up");
    println!("  ip link set DEVICE down      - bring device down");
    println!("  ip link set DEVICE name NAME - rename device");
    println!("  ip link add NAME type TYPE   - add virtual interface");
    println!("  ip link del DEVICE           - delete device");
    println!();
    println!("Help:");
    println!("  ip link help                 - display this help message");
}

fn do_show(iface: Option<&str>) -> i32 {
    match link_show(iface) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("ip link: {}", e);
            1
        }
    }
}

fn link_show(iface: Option<&str>) -> Result<(), String> {
    let net_dir = Path::new("/sys/class/net");

    let interfaces: Vec<_> = if let Some(name) = iface {
        vec![name.to_string()]
    } else {
        match fs::read_dir(net_dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect(),
            Err(e) => return Err(format!("failed to read /sys/class/net: {}", e)),
        }
    };

    let mut interfaces: Vec<(u32, String)> = interfaces
        .into_iter()
        .map(|name| (get_ifindex(&name), name))
        .collect();
    interfaces.sort_by_key(|(idx, name)| (*idx, name.clone()));

    for (ifindex, name) in interfaces {
        let path = net_dir.join(&name);

        let mtu = fs::read_to_string(path.join("mtu"))
            .unwrap_or_else(|_| "0".to_string())
            .trim()
            .to_string();

        let addr = fs::read_to_string(path.join("address"))
            .unwrap_or_else(|_| "".to_string())
            .trim()
            .to_string();

        let broadcast = fs::read_to_string(path.join("broadcast"))
            .unwrap_or_else(|_| "".to_string())
            .trim()
            .to_string();

        let flags_str = fs::read_to_string(path.join("flags"))
            .unwrap_or_else(|_| "0".to_string())
            .trim()
            .trim_start_matches("0x")
            .to_string();

        let flags = u64::from_str_radix(&flags_str, 16).unwrap_or(0);

        let state = if flags & 0x1 != 0 { "UP" } else { "DOWN" };

        println!("{}: {} mtu {} state {}", ifindex, name, mtu, state);

        if !addr.is_empty() {
            let flags_str = format_flags(flags);
            if !flags_str.is_empty() {
                println!("    {}", flags_str);
            }
            println!("    link/ether {} brd {}", addr, broadcast);
        }
    }

    Ok(())
}

fn format_flags(flags: u64) -> String {
    let mut v = Vec::new();
    if flags & 0x1 != 0 {
        v.push("UP");
    }
    if flags & 0x2 != 0 {
        v.push("LOWER_UP");
    }
    if flags & 0x4 != 0 {
        v.push("DORMANT");
    }
    if flags & 0x8 != 0 {
        v.push("RUNNING");
    }
    if flags & 0x10 != 0 {
        v.push("MULTICAST");
    }
    if flags & 0x100 != 0 {
        v.push("BROADCAST");
    }
    if flags & 0x200 != 0 {
        v.push("POINTOPOINT");
    }
    if flags & 0x1000 != 0 {
        v.push("LOOPBACK");
    }
    if flags & 0x2000 != 0 {
        v.push("PROMISC");
    }
    if flags & 0x4000 != 0 {
        v.push("ALLMULTI");
    }
    if flags & 0x8 != 0 {
        v.push("MASTER");
    }
    if flags & 0x4 != 0 {
        v.push("SLAVE");
    }
    v.join(" ")
}

fn do_set(args: &[String]) -> i32 {
    if args.len() < 3 {
        eprintln!("ip link set: usage: ip link set DEVICE up|down|name NAME");
        return 1;
    }

    let device = &args[1];
    let action = &args[2];

    let result = match action.as_str() {
        "up" => link_set_up(device),
        "down" => link_set_down(device),
        "name" => {
            if args.len() < 4 {
                eprintln!("ip link set: usage: ip link set DEVICE name NAME");
                return 1;
            }
            let new_name = &args[3];
            link_set_name(device, new_name)
        }
        "mtu" => {
            if args.len() < 4 {
                eprintln!("ip link set: usage: ip link set DEVICE mtu MTU");
                return 1;
            }
            let mtu: u32 = match args[3].parse() {
                Ok(m) => m,
                Err(_) => {
                    eprintln!("ip link set: invalid MTU value");
                    return 1;
                }
            };
            link_set_mtu(device, mtu)
        }
        _ => {
            eprintln!(
                "ip link set: invalid action '{}', use up/down/name/mtu",
                action
            );
            return 1;
        }
    };

    match result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("ip link set: {}", e);
            1
        }
    }
}

fn link_set_up(device: &str) -> Result<(), String> {
    set_link_state(device, true)
}

fn link_set_down(device: &str) -> Result<(), String> {
    set_link_state(device, false)
}

fn set_link_state(device: &str, up: bool) -> Result<(), String> {
    let index = get_ifindex(device);
    if index == 0 {
        return Err(format!("device '{}' not found", device));
    }

    let mut sock = NetlinkSocket::connect().map_err(|e| format!("netlink socket: {}", e))?;

    let ifinfo = IfInfoMsg {
        ifi_family: 0,
        ifi_pad: 0,
        ifi_type: 0,
        ifi_index: index as i32,
        ifi_flags: if up { IFF_UP } else { 0 },
        ifi_change: IFF_UP,
    };

    let mut payload = Vec::new();
    put_struct(&mut payload, &ifinfo);

    send_netlink(
        sock.fd(),
        &payload,
        RTM_NEWLINK,
        NLM_F_REQUEST | NLM_F_ACK,
        sock.next_seq(),
    )
    .map_err(|e| format!("send netlink: {}", e))?;

    Ok(())
}

fn link_set_name(device: &str, new_name: &str) -> Result<(), String> {
    let _ = (device, new_name);
    Err("rename not supported yet".to_string())
}

fn link_set_mtu(device: &str, mtu: u32) -> Result<(), String> {
    let _ = (device, mtu);
    Err("setting MTU is not supported yet".to_string())
}

fn do_add(args: &[String]) -> i32 {
    if args.len() < 3 {
        eprintln!("ip link add: usage: ip link add NAME type TYPE");
        return 1;
    }

    eprintln!("ip link add: not supported yet");
    1
}

fn do_del(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("ip link del: usage: ip link del DEVICE");
        return 1;
    }

    eprintln!("ip link del: not supported yet");
    1
}

fn get_ifindex(name: &str) -> u32 {
    let path = Path::new("/sys/class/net").join(name).join("ifindex");
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}
