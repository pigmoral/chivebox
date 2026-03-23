mod completion;
mod input;
mod shell;

use std::env;
use std::ffi::CString;

use input::{BasicLineEditor, ReadOutcome};
use shell::ShellState;

fn acquire_controlling_terminal() {
    unsafe {
        // Check if we already have a controlling terminal
        // tcgetpgrp returns -1 with errno ENOTTY if no controlling terminal
        if libc::tcgetpgrp(0) > 0 {
            return;
        }

        // Create a new session and become session leader
        let sid = libc::setsid();
        if sid < 0 {
            return;
        }

        // Open terminal and make it controlling
        for tty in ["/dev/console", "/dev/ttyS0"] {
            if let Ok(tty_c) = CString::new(tty) {
                let fd = libc::open(tty_c.as_ptr(), libc::O_RDWR);
                if fd >= 0 {
                    // Make this terminal our controlling terminal
                    libc::ioctl(fd, libc::TIOCSCTTY, 0);
                    libc::dup2(fd, 0);
                    libc::dup2(fd, 1);
                    libc::dup2(fd, 2);
                    if fd > 2 {
                        libc::close(fd);
                    }
                    break;
                }
            }
        }
    }
}

pub fn run_shell() -> i32 {
    acquire_controlling_terminal();

    // Set up SIGINT handler
    shell::setup_sigint_handler();
    unsafe {
        libc::signal(libc::SIGQUIT, libc::SIG_DFL);
    }

    shell::ensure_default_path();

    let cwd = env::current_dir().unwrap_or_else(|_| std::path::Path::new("/").to_path_buf());
    let mut state = ShellState::new(cwd);
    let mut editor = BasicLineEditor::new();
    let mut pending = String::new();

    loop {
        // Check for pending SIGINT from previous command
        if shell::check_sigint() {
            println!("^C");
            state.last_status = 130;
        }

        let prompt = if pending.is_empty() {
            shell::prompt(&state)
        } else {
            shell::continuation_prompt()
        };

        match editor.read_line(&prompt, |line, pos| {
            completion::complete(line, pos, &state.cwd)
        }) {
            Ok(ReadOutcome::Line(line)) => {
                let full_line = if pending.is_empty() {
                    line
                } else {
                    format!("{pending}\n{line}")
                };

                if shell::is_incomplete_input(&full_line) {
                    pending = full_line;
                    continue;
                }

                pending.clear();

                if !full_line.trim().is_empty() {
                    editor.add_to_history(&full_line);
                }

                let status = match shell::execute_line(&full_line, &mut state) {
                    Ok(status) => status,
                    Err(err) => {
                        eprintln!("sh: {err}");
                        2
                    }
                };

                state.last_status = status;
                if let Some(code) = state.exit_code {
                    return code;
                }
            }
            Ok(ReadOutcome::Interrupted) => {
                pending.clear();
                println!("^C");
                state.last_status = 130;
            }
            Ok(ReadOutcome::Eof) => {
                if pending.is_empty() {
                    println!("exit");
                    return state.last_status;
                }

                pending.clear();
                state.last_status = 130;
            }
            Err(err) => {
                eprintln!("readline error: {err}");
                return 1;
            }
        }
    }
}

#[cfg(test)]
mod tests;
