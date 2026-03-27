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
        "a" | "addr" => addr::main(&args[1..]),
        "l" | "link" => link::main(&args[1..]),
        "r" | "route" => route::main(&args[1..]),
        "n" | "neigh" => neigh::main(&args[1..]),
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
    println!("  link          - network device configuration");
    println!("  addr          - protocol address management");
    println!("  route         - routing table management");
    println!("  neigh         - neighbor table management");
    println!("  rule            - routing policy database");
    println!();
    println!("Options:");
    println!("  -family       specify protocol family (inet, inet6, link)");
    println!("  -oneline      output in single line format");
    println!();
    println!("Help:");
    println!("  ip help         - display this help message");
}
