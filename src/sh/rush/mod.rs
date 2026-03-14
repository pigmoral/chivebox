mod completion;
mod input;
mod shell;

use std::env;

use input::{BasicLineEditor, ReadOutcome};
use shell::ShellState;

pub fn run_shell() -> i32 {
    shell::ensure_default_path();

    let cwd = env::current_dir().unwrap_or_else(|_| std::path::Path::new("/").to_path_buf());
    let mut state = ShellState::new(cwd);
    let mut editor = BasicLineEditor::new();
    let mut pending = String::new();

    loop {
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
