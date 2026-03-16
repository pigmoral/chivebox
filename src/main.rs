use std::env;
use std::process;

mod applets;
pub mod blkid;
pub mod init;
pub mod mount;
pub mod sh;
pub mod umount;
pub mod volume_id;

fn main() {
    uucore::panic::mute_sigpipe_panic();

    let args: Vec<String> = env::args().collect();

    let binary_name = args
        .first()
        .and_then(|p| std::path::Path::new(p).file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("chivebox");

    let args_iter = env::args_os().skip(1);

    // symlink mode: ./ls -> argv[0] = "./ls", binary_name = "ls"
    // normal mode: ./chivebox ls -> argv[0] = "./chivebox", argv[1] = "ls"
    let cmd = if applets::find_applet(binary_name).is_some() {
        binary_name
    } else if args.len() > 1 {
        args[1].as_str()
    } else {
        ""
    };

    match cmd {
        "--help" | "-h" => {
            usage();
            process::exit(0);
        }
        "--version" | "-V" => {
            println!("chivebox 0.1.0");
            process::exit(0);
        }
        "--list" | "-l" => {
            list_commands();
            process::exit(0);
        }
        _ => {
            if let Some(applet) = applets::find_applet(cmd) {
                process::exit((applet.main)(args_iter));
            }
            if args.len() < 2 {
                usage();
                process::exit(0);
            }
            eprintln!("chivebox: {}: not found", cmd);
            process::exit(127);
        }
    }
}

fn usage() {
    println!("chivebox 0.1.0");
    println!("Usage: chivebox [FUNCTION] [ARGS...]");

    println!();
    println!("Functions:");
    print!("        ");
    let mut line_len = 8;
    for applet in applets::list_applets() {
        let name_len = applet.name.len() + 1;
        if line_len + name_len > 80 {
            println!();
            print!("        ");
            line_len = 8;
        }
        print!("{} ", applet.name);
        line_len += name_len;
    }
    println!();

    println!();
    println!("Options:");
    println!("    --help      Show this help");
    println!("    --version   Show version");
    println!("    --list      List all available commands");
}

fn list_commands() {
    for applet in applets::list_applets() {
        println!("{:8} {}", applet.name, applet.help);
    }
}
