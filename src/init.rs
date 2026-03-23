use std::env;
use std::process;
use std::thread;
use std::time::Duration;

use crate::applets::AppletArgs;

pub fn main(_args: AppletArgs) -> i32 {
    let pid = unsafe { libc::getpid() };
    if pid != 1 {
        eprintln!("init: only intended to be run as PID 1");
        return 1;
    }

    init_signals();

    console_init();

    println!("ChiveBox 0.1.0");

    mount_filesystems();

    set_env();

    loop {
        let mut child = process::Command::new("/bin/sh")
            .stdin(process::Stdio::inherit())
            .stdout(process::Stdio::inherit())
            .stderr(process::Stdio::inherit())
            .spawn();

        match child {
            Ok(ref mut c) => {
                let _ = c.wait();
            }
            Err(e) => {
                eprintln!("Shell error: {}", e);
            }
        }

        thread::sleep(Duration::from_secs(2));
    }
}

fn init_signals() {
    use libc::*;

    unsafe {
        signal(SIGINT, SIG_IGN);
        signal(SIGQUIT, SIG_IGN);
        signal(SIGTERM, SIG_IGN);
        signal(SIGTSTP, SIG_IGN);
        // Do NOT ignore SIGCHLD - we need it for wait() to work
        signal(SIGHUP, SIG_IGN);
        signal(SIGUSR1, SIG_IGN);
        signal(SIGUSR2, SIG_IGN);
    }
}

fn console_init() {
    use std::ffi::CString;

    unsafe {
        let console = CString::new("/dev/console").unwrap();
        let fd = libc::open(console.as_ptr(), libc::O_RDWR);
        if fd >= 0 {
            libc::dup2(fd, libc::STDIN_FILENO);
            libc::dup2(fd, libc::STDOUT_FILENO);
            libc::dup2(fd, libc::STDERR_FILENO);
            if fd > 2 {
                libc::close(fd);
            }
        }
        // Try to make stdin the controlling terminal
        libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY, 0);
    }
}

fn mount_filesystems() {
    unsafe {
        let _ = mount_internal("proc", "/proc", "proc", 0);
        let _ = mount_internal("dev", "/dev", "devtmpfs", 0);
        let _ = mount_internal("sys", "/sys", "sysfs", 0);
        let _ = mount_internal("tmpfs", "/tmp", "tmpfs", 0);
    }
}

unsafe fn mount_internal(source: &str, target: &str, fstype: &str, flags: libc::c_ulong) -> i32 {
    use std::ffi::CString;

    let source_c = CString::new(source).unwrap();
    let target_c = CString::new(target).unwrap();
    let fstype_c = CString::new(fstype).unwrap();

    unsafe {
        libc::mount(
            source_c.as_ptr(),
            target_c.as_ptr(),
            fstype_c.as_ptr(),
            flags,
            std::ptr::null(),
        )
    }
}

fn set_env() {
    unsafe {
        env::set_var("PATH", "/bin:/sbin:/usr/bin:/usr/sbin");
        env::set_var("SHELL", "/bin/sh");
        env::set_var("USER", "root");
        env::set_var("TERM", "vt100");
    }
}
