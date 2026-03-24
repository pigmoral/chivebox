pub mod rush;

use std::ffi::OsString;
use std::path::PathBuf;

use crate::applets::AppletArgs;

const HELP_TEXT: &str = r#"Usage: sh [OPTIONS] [SCRIPT_FILE]

Options:
  -c COMMAND    Execute COMMAND and exit
  -h, --help    Display this help message

If no options are provided, start an interactive shell.
"#;

pub fn main(args: AppletArgs) -> i32 {
    let args: Vec<OsString> = args.collect();

    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{}", HELP_TEXT);
        return 0;
    }

    if let Some(pos) = args.iter().position(|arg| arg == "-c") {
        if pos + 1 >= args.len() {
            eprintln!("sh: -c requires an argument");
            return 2;
        }

        let command = args[pos + 1].to_str().unwrap_or("");

        if command.is_empty() {
            eprintln!("sh: -c requires an argument");
            return 2;
        }

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let mut state = rush::shell::ShellState::new(cwd);
        rush::shell::ensure_default_path();

        match rush::shell::execute_line(&command, &mut state) {
            Ok(status) => status,
            Err(err) => {
                eprintln!("sh: {err}");
                2
            }
        }
    } else {
        rush::run_shell()
    }
}
