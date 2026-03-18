use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::iter;

const MNT_DETACH: libc::c_ulong = 0x00000002;

pub fn main(_args: iter::Skip<env::ArgsOs>) -> i32 {
    let mut force = false;
    let mut lazy = false;
    let mut read_only = false;
    let mut verbose = false;
    let mut unmount_all = false;
    let mut fstype: Option<String> = None;

    let args_vec: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args_vec.len() {
        match args_vec[i].as_str() {
            "-h" | "--help" => {
                println!(
                    "Usage: umount [-f] [-l] [-r] [-v] [-a] [-n] [-t type] device|mount_point"
                );
                println!("  -f, --force      Force unmount");
                println!("  -l, --lazy       Lazy unmount (detach)");
                println!("  -r, --read-only  Remount read-only if busy");
                println!("  -v, --verbose    Verbose");
                println!("  -a, --all        Unmount all filesystems");
                println!("  -n, --no-mtab    Don't write to /etc/mtab");
                println!("  -t, --types      Unmount only these filesystem types");
                return 0;
            }
            "-f" | "--force" => force = true,
            "-l" | "--lazy" => lazy = true,
            "-r" | "--read-only" => read_only = true,
            "-v" | "--verbose" => verbose = true,
            "-a" | "--all" => unmount_all = true,
            "-t" | "--types" => {
                if i + 1 < args_vec.len() {
                    fstype = Some(args_vec[i + 1].clone());
                    i += 1;
                }
            }
            _ => {
                if args_vec[i].starts_with('-') {
                    eprintln!("Unknown option: {}", args_vec[i]);
                    return 1;
                }
            }
        }
        i += 1;
    }

    let targets: Vec<String> = args_vec[1..]
        .iter()
        .filter(|s| !s.starts_with('-'))
        .cloned()
        .collect();

    if targets.is_empty() && !unmount_all {
        eprintln!("Usage: umount [-f] [-l] [-r] [-v] [-a] [-n] [-t type] device|mount_point");
        return 1;
    }

    let mounts = match get_mounts() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to read mounts: {}", e);
            return 1;
        }
    };

    let mut flags: libc::c_ulong = 0;
    if force {
        flags |= libc::MNT_FORCE as libc::c_ulong;
    }
    if lazy {
        flags |= MNT_DETACH;
    }

    let mut failed = false;

    if unmount_all {
        for mount in mounts.iter().rev() {
            if let Some(ref ft) = fstype {
                if &mount.fstype != ft {
                    continue;
                }
            }

            if unmount_single(&mount.target, flags, read_only, verbose) != 0 {
                failed = true;
            }
        }
    } else {
        for target in &targets {
            let target_trimmed = target.trim_end_matches('/');
            let mut found = false;
            for mount in &mounts {
                let mount_target_trimmed = mount.target.trim_end_matches('/');
                if mount_target_trimmed == target_trimmed || mount.source == *target {
                    if let Some(ref ft) = fstype {
                        if &mount.fstype != ft {
                            continue;
                        }
                    }
                    if unmount_single(&mount.target, flags, read_only, verbose) != 0 {
                        failed = true;
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                eprintln!("umount: {}: not mounted", target);
                failed = true;
            }
        }
    }

    if failed { 1 } else { 0 }
}

fn unmount_single(path: &str, flags: libc::c_ulong, read_only: bool, verbose: bool) -> i32 {
    let result =
        unsafe { libc::umount2(path.as_ptr() as *const libc::c_char, flags as libc::c_int) };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        if read_only && errno.raw_os_error() == Some(libc::EBUSY) {
            if verbose {
                eprintln!("{} busy - remounting read-only", path);
            }
            unsafe {
                libc::mount(
                    std::ptr::null(),
                    path.as_ptr() as *const libc::c_char,
                    std::ptr::null(),
                    libc::MS_REMOUNT | libc::MS_RDONLY,
                    std::ptr::null(),
                )
            };
            return 0;
        }
        eprintln!("umount: {}: {}", path, errno);
        return 1;
    }

    if verbose {
        println!("{}", path);
    }
    0
}

struct MountInfo {
    source: String,
    target: String,
    fstype: String,
}

fn get_mounts() -> std::io::Result<Vec<MountInfo>> {
    let mut mounts = Vec::new();

    let file = File::open("/proc/self/mountinfo")?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let target = parts[4].to_string();
            let fstype = parts[2].to_string();
            let source = if parts.len() >= 5 && parts[0] != "0" {
                parts[0].to_string()
            } else {
                String::new()
            };

            mounts.push(MountInfo {
                source,
                target,
                fstype,
            });
        }
    }

    Ok(mounts)
}
