use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::applets::AppletArgs;

pub fn main(args: AppletArgs) -> i32 {
    // Skip argv[0] (applet name)
    let args: Vec<String> = args.skip(1).filter_map(|s| s.into_string().ok()).collect();

    let mut show_all = false;
    let mut commands: Vec<&String> = Vec::new();

    for arg in args.iter() {
        if arg == "-a" {
            show_all = true;
        } else if arg == "--" {
            continue;
        } else if arg.starts_with('-') {
            eprintln!("which: invalid option '{}'", arg);
            return 1;
        } else {
            commands.push(arg);
        }
    }

    if commands.is_empty() {
        eprintln!("which: no command specified");
        return 1;
    }

    let path_var = env::var("PATH").unwrap_or_else(|_| "/bin:/usr/bin".to_string());
    let paths: Vec<&str> = path_var.split(':').collect();

    let mut status = 0;
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for cmd in commands {
        if cmd.contains('/') {
            if is_executable(Path::new(cmd)) {
                writeln!(stdout, "{}", cmd).unwrap();
            } else {
                status = 1;
            }
        } else {
            let mut found = false;
            for dir in &paths {
                let full_path = Path::new(dir).join(cmd);
                if is_executable(&full_path) {
                    writeln!(stdout, "{}", full_path.display()).unwrap();
                    found = true;
                    if !show_all {
                        break;
                    }
                }
            }
            if !found {
                status = 1;
            }
        }
    }

    status
}

fn is_executable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    match fs::metadata(path) {
        Ok(meta) => {
            let mode = meta.permissions().mode();
            mode & 0o111 != 0
        }
        Err(_) => false,
    }
}
