use crate::applets::AppletArgs;

pub mod addr;
pub mod link;
pub mod neigh;
pub mod netlink;
pub mod route;
pub mod rule;

pub fn main(args: AppletArgs) -> i32 {
    let args: Vec<String> = args
        .skip(1)
        .map(|s| s.to_str().unwrap_or("").to_string())
        .collect();

    if args.is_empty() {
        print_help();
        return 0;
    }

    match args[0].as_str() {
        "a" => addr::main(&args[1..]),
        "l" => link::main(&args[1..]),
        "r" => route::main(&args[1..]),
        "n" => neigh::main(&args[1..]),
        "link" => link::main(&args[1..]),
        "addr" => addr::main(&args[1..]),
        "route" => route::main(&args[1..]),
        "neigh" => neigh::main(&args[1..]),
        "rule" => rule::main(&args[1..]),
        "-h" | "--help" | "help" => {
            print_help();
            0
        }
        _ => {
            eprintln!("ip: '{}' is not a valid command", args[0]);
            print_help();
            1
        }
    }
}

fn print_help() {
    println!("Usage: ip [OPTIONS] OBJECT ...");
    println!();
    println!("Objects:");
    println!("  ip link     - network device configuration");
    println!("  ip addr     - protocol address management");
    println!("  ip route    - routing table management");
    println!("  ip neigh    - neighbor table management");
    println!("  ip rule     - routing policy database");
    println!();
    println!("Options:");
    println!("  -f[amily]   specify protocol family (inet, inet6, link)");
    println!("  -o[neline]   output in single line format");
    println!();
    println!("Aliases:");
    println!("  ip a, ip l, ip r, ip n");
}
