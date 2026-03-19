use std::fs;

pub fn main(args: &[String]) -> i32 {
    if args.is_empty() {
        return do_list();
    }

    match args[0].as_str() {
        "list" | "show" => do_list(),
        "add" => do_add(args),
        "del" | "delete" => do_del(args),
        "help" | "-h" | "--help" => {
            print_help();
            0
        }
        _ => {
            eprintln!("ip rule: '{}' is not a valid command", args[0]);
            1
        }
    }
}

fn print_help() {
    println!("Usage: ip rule COMMAND");
    println!();
    println!("Commands:");
    println!("  ip rule list   show routing policy rules");
    println!("  ip rule add    add rule (not fully supported)");
    println!("  ip rule del    delete rule (not fully supported)");
}

fn do_list() -> i32 {
    match fs::read_to_string("/etc/iproute2/rt_tables") {
        Ok(text) => {
            print!("{}", text);
            0
        }
        Err(_) => {
            eprintln!("ip rule: not supported in this build yet");
            1
        }
    }
}

fn do_add(_args: &[String]) -> i32 {
    eprintln!("ip rule add: not supported in this build yet");
    1
}

fn do_del(_args: &[String]) -> i32 {
    eprintln!("ip rule del: not supported in this build yet");
    1
}
