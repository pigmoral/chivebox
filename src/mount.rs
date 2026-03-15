use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::iter;

pub fn main(args: iter::Skip<env::ArgsOs>) -> i32 {
    let args: Vec<String> = args
        .skip(1) // Skip program name (e.g., "mount")
        .map(|s| s.to_str().unwrap_or("").to_string())
        .collect();

    // Show help if -h or --help
    if !args.is_empty() && (args[0] == "-h" || args[0] == "--help") {
        print_usage();
        return 0;
    }

    // Show mounts if no arguments
    if args.is_empty() {
        return show_mounts();
    }

    let mut fs_type = "";
    let mut options = "";
    let mut readonly = false;
    let mut verbose = false;
    let mut no_mtab = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-t" => {
                if i + 1 < args.len() {
                    fs_type = &args[i + 1];
                    i += 2;
                } else {
                    eprintln!("mount: -t requires an argument");
                    return 1;
                }
            }
            "-o" => {
                if i + 1 < args.len() {
                    options = &args[i + 1];
                    i += 2;
                } else {
                    eprintln!("mount: -o requires an argument");
                    return 1;
                }
            }
            "-r" => {
                readonly = true;
                i += 1;
            }
            "-w" => {
                readonly = false;
                i += 1;
            }
            "-n" => {
                no_mtab = true;
                i += 1;
            }
            "-v" => {
                verbose = true;
                i += 1;
            }
            "-a" => {
                eprintln!("mount: -a not supported (no /etc/fstab)");
                return 1;
            }
            _ => break,
        }
    }

    let remaining: Vec<&str> = args[i..].iter().map(|s| s.as_str()).collect();

    if remaining.is_empty() {
        return show_mounts();
    }

    if remaining.len() < 2 {
        eprintln!("mount: missing target");
        return 1;
    }

    let source = remaining[0];
    let target = remaining[1];

    if source.is_empty()
        && fs_type != "tmpfs"
        && fs_type != "proc"
        && fs_type != "sysfs"
        && fs_type != "devtmpfs"
    {
        eprintln!("mount: missing source");
        return 1;
    }

    mount_fs(source, target, fs_type, options, readonly, verbose, no_mtab)
}

fn print_usage() {
    println!("Usage: mount [-t type] [-o options] [-rn] [-v] source target");
    println!("Mount a filesystem");
    println!();
    println!("  -t      filesystem type (proc, sysfs, devtmpfs, tmpfs, ext4, ...)");
    println!("  -o      mount options (ro, rw, remount, nosuid, nodev, noexec, ...)");
    println!("  -r      mount read-only");
    println!("  -w      mount read-write (default)");
    println!("  -n      don't write /etc/mtab");
    println!("  -v      verbose");
    println!();
    println!("With no arguments, shows current mounts.");
}

fn show_mounts() -> i32 {
    let paths = [
        "/proc/self/mountinfo",
        "/proc/self/mounts",
        "/proc/mounts",
        "/etc/mtab",
    ];

    for path in &paths {
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                println!("{}", line);
            }
            return 0;
        }
    }

    eprintln!("mount: cannot read mount table");
    1
}

#[cfg(unix)]
fn mount_fs(
    source: &str,
    target: &str,
    fs_type: &str,
    options: &str,
    readonly: bool,
    verbose: bool,
    _no_mtab: bool,
) -> i32 {
    use std::ffi::CString;

    let mut flags: libc::c_ulong = 0;

    if readonly {
        flags |= libc::MS_RDONLY;
    }

    for opt in options.split(',') {
        match opt {
            "ro" => flags |= libc::MS_RDONLY,
            "rw" => flags &= !libc::MS_RDONLY,
            "remount" => flags |= libc::MS_REMOUNT,
            "nosuid" => flags |= libc::MS_NOSUID,
            "suid" => flags &= !libc::MS_NOSUID,
            "nodev" => flags |= libc::MS_NODEV,
            "dev" => flags &= !libc::MS_NODEV,
            "noexec" => flags |= libc::MS_NOEXEC,
            "exec" => flags &= !libc::MS_NOEXEC,
            "sync" => flags |= libc::MS_SYNCHRONOUS,
            "async" => flags &= !libc::MS_SYNCHRONOUS,
            "atime" => flags &= !libc::MS_NOATIME,
            "noatime" => flags |= libc::MS_NOATIME,
            "diratime" => flags &= !libc::MS_NODIRATIME,
            "nodiratime" => flags |= libc::MS_NODIRATIME,
            "bind" => flags |= libc::MS_BIND,
            "move" => flags |= libc::MS_MOVE,
            "rec" => flags |= libc::MS_REC,
            _ => {}
        }
    }

    let source_c = CString::new(if source.is_empty() { "" } else { source }).unwrap();
    let target_c = CString::new(target).unwrap();
    let fstype_c = CString::new(if fs_type.is_empty() { "" } else { fs_type }).unwrap();
    let data_c = CString::new(options).unwrap();

    let result = unsafe {
        libc::mount(
            source_c.as_ptr() as *const libc::c_char,
            target_c.as_ptr() as *const libc::c_char,
            fstype_c.as_ptr() as *const libc::c_char,
            flags,
            data_c.as_ptr() as *const libc::c_void,
        )
    };

    if result != 0 {
        let err = std::io::Error::last_os_error();
        eprintln!("mount: {}: {}", target, err);
        return 1;
    }

    if verbose {
        println!("mount: {} mounted on {}", source, target);
    }

    0
}
