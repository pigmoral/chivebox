use std::env;
use std::ffi::OsString;
use std::process;

mod applets;
pub mod blkid;
pub mod init;
pub mod mount;
pub mod sh;
pub mod umount;
pub mod volume_id;
pub mod which;

fn main() {
    uucore::panic::mute_sigpipe_panic();

    let raw_args: Vec<OsString> = env::args_os().collect();

    // Determine command and normalize argv
    let (cmd, applet_argv) = match prepare_applet_args(&raw_args) {
        Some(result) => result,
        None => {
            usage();
            process::exit(0);
        }
    };

    match cmd.as_str() {
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
            if let Some(applet) = applets::find_applet(&cmd) {
                process::exit((applet.main)(applet_argv.into_iter()));
            }
            eprintln!("chivebox: {}: not found", cmd);
            process::exit(127);
        }
    }
}

/// Prepare applet arguments from raw command line arguments.
///
/// Returns `Some(applet_argv)` or `None` if no command specified.
///
/// Behavior:
/// - `/bin/echo hello` -> argv ["/bin/echo", "hello"]
/// - `chivebox echo hello` -> argv ["echo", "hello"]
/// - `/bin/chivebox echo hello` -> argv ["echo", "hello"]
fn prepare_applet_args(raw_args: &[OsString]) -> Option<(String, Vec<OsString>)> {
    if raw_args.is_empty() {
        return None;
    }

    let argv0 = raw_args[0].to_str().unwrap_or("");
    let binary_name = std::path::Path::new(argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("chivebox");

    // symlink mode: argv[0] basename is an applet name
    if let Some(applet) = applets::find_applet(binary_name) {
        return Some((applet.name.to_string(), raw_args.to_vec()));
    }

    // normal mode: chivebox APPLET args...
    if raw_args.len() < 2 {
        return None;
    }

    let cmd = raw_args[1].to_str().unwrap_or("");

    // Handle special options
    if cmd == "--help"
        || cmd == "-h"
        || cmd == "--version"
        || cmd == "-V"
        || cmd == "--list"
        || cmd == "-l"
    {
        return Some((cmd.to_string(), raw_args[1..].to_vec()));
    }

    // Check if it's a valid applet
    if let Some(applet) = applets::find_applet(cmd) {
        // Rewrite argv: [APPLET, args...]
        let mut applet_argv = vec![OsString::from(applet.name)];
        applet_argv.extend(raw_args[2..].iter().cloned());
        return Some((applet.name.to_string(), applet_argv));
    }

    // Unknown command
    Some((cmd.to_string(), raw_args[1..].to_vec()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(|s| OsString::from(s)).collect()
    }

    #[test]
    fn test_symlink_mode_with_path() {
        // /bin/echo hello -> cmd "echo", argv ["/bin/echo", "hello"]
        let raw_args = os_args(&["/bin/echo", "hello"]);
        let result = prepare_applet_args(&raw_args).unwrap();
        assert_eq!(result.0, "echo");
        assert_eq!(result.1, os_args(&["/bin/echo", "hello"]));
    }

    #[test]
    fn test_symlink_mode_without_path() {
        // echo hello -> cmd "echo", argv ["echo", "hello"]
        let raw_args = os_args(&["echo", "hello"]);
        let result = prepare_applet_args(&raw_args).unwrap();
        assert_eq!(result.0, "echo");
        assert_eq!(result.1, os_args(&["echo", "hello"]));
    }

    #[test]
    fn test_normal_mode() {
        // chivebox echo hello -> cmd "echo", argv ["echo", "hello"]
        let raw_args = os_args(&["chivebox", "echo", "hello"]);
        let result = prepare_applet_args(&raw_args).unwrap();
        assert_eq!(result.0, "echo");
        assert_eq!(result.1, os_args(&["echo", "hello"]));
    }

    #[test]
    fn test_normal_mode_with_path() {
        // /bin/chivebox echo hello -> cmd "echo", argv ["echo", "hello"]
        let raw_args = os_args(&["/bin/chivebox", "echo", "hello"]);
        let result = prepare_applet_args(&raw_args).unwrap();
        assert_eq!(result.0, "echo");
        assert_eq!(result.1, os_args(&["echo", "hello"]));
    }

    #[test]
    fn test_no_args() {
        // [] -> None
        let raw_args: Vec<OsString> = vec![];
        assert!(prepare_applet_args(&raw_args).is_none());
    }

    #[test]
    fn test_only_program_name() {
        // ["chivebox"] -> None
        let raw_args = os_args(&["chivebox"]);
        assert!(prepare_applet_args(&raw_args).is_none());
    }

    #[test]
    fn test_help_option() {
        // chivebox --help -> cmd "--help"
        let raw_args = os_args(&["chivebox", "--help"]);
        let result = prepare_applet_args(&raw_args).unwrap();
        assert_eq!(result.0, "--help");
    }

    #[test]
    fn test_unknown_command() {
        // chivebox unknowncmd -> cmd "unknowncmd"
        let raw_args = os_args(&["chivebox", "unknowncmd"]);
        let result = prepare_applet_args(&raw_args).unwrap();
        assert_eq!(result.0, "unknowncmd");
    }
}
