use std::io::{self, Write};
use std::os::fd::RawFd;

use libc::{
    ECHO, ICANON, ISIG, TCSANOW, TIOCGWINSZ, VMIN, VTIME, c_void, ioctl, isatty, read, tcgetattr,
    tcsetattr, termios, winsize,
};

use super::completion::{CompletionEntry, apply_completion, display_name, token_span};

const MAX_HISTORY: usize = 100;

pub(crate) enum ReadOutcome {
    Line(String),
    Eof,
    Interrupted,
}

enum KeyCode {
    Up,
    Down,
    Left,
    Right,
}

pub(crate) struct BasicLineEditor {
    history: Vec<String>,
    history_index: usize,
    saved_line: String,
}

impl BasicLineEditor {
    pub(crate) fn new() -> Self {
        Self {
            history: Vec::new(),
            history_index: 0,
            saved_line: String::new(),
        }
    }

    pub(crate) fn add_to_history(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        if let Some(last) = self.history.last() {
            if last == line {
                return;
            }
        }
        if self.history.len() >= MAX_HISTORY {
            self.history.remove(0);
        }
        self.history.push(line.to_string());
    }

    pub(crate) fn read_line<F>(&mut self, prompt: &str, mut completer: F) -> io::Result<ReadOutcome>
    where
        F: FnMut(&str, usize) -> Vec<CompletionEntry>,
    {
        if stdin_is_tty() {
            self.read_line_tty(prompt, &mut completer)
        } else {
            self.read_line_non_tty(prompt)
        }
    }

    fn read_line_non_tty(&mut self, _prompt: &str) -> io::Result<ReadOutcome> {
        let mut input = String::new();

        loop {
            match io::stdin().read_line(&mut input) {
                Ok(0) => {
                    if input.is_empty() {
                        return Ok(ReadOutcome::Eof);
                    }
                    break;
                }
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }

        Ok(ReadOutcome::Line(
            input.trim_end_matches(['\n', '\r']).to_string(),
        ))
    }

    fn read_line_tty<F>(&mut self, prompt: &str, completer: &mut F) -> io::Result<ReadOutcome>
    where
        F: FnMut(&str, usize) -> Vec<CompletionEntry>,
    {
        let _guard = RawModeGuard::new(0)?;
        let mut line = String::new();
        let mut cursor: usize = 0;
        let mut stdout = io::stdout();

        self.history_index = self.history.len();
        self.saved_line.clear();

        print_prompt(&mut stdout, prompt)?;

        loop {
            let byte = read_byte(0)?;
            match byte {
                b'\r' | b'\n' => {
                    write_newline(&mut stdout)?;
                    return Ok(ReadOutcome::Line(line));
                }
                3 => {
                    stdout.write_all(b"^C")?;
                    write_newline(&mut stdout)?;
                    return Ok(ReadOutcome::Interrupted);
                }
                4 => {
                    if line.is_empty() {
                        write_newline(&mut stdout)?;
                        return Ok(ReadOutcome::Eof);
                    }
                }
                8 | 127 => {
                    if cursor > 0 {
                        cursor -= 1;
                        line.remove(cursor);
                        redraw_line(&mut stdout, prompt, &line, cursor)?;
                    }
                }
                b'\t' => {
                    let completions = completer(&line, cursor);
                    handle_completions(&mut stdout, prompt, &mut line, &mut cursor, completions)?;
                }
                12 => {
                    clear_screen(&mut stdout)?;
                    print_prompt(&mut stdout, prompt)?;
                    stdout.write_all(line.as_bytes())?;
                    if cursor < line.len() {
                        move_cursor_left(&mut stdout, line.len() - cursor)?;
                    }
                    stdout.flush()?;
                }
                0x1b => {
                    if let Some(key) = parse_escape_sequence(0)? {
                        match key {
                            KeyCode::Up => {
                                self.handle_history_up(
                                    &mut line,
                                    &mut cursor,
                                    &mut stdout,
                                    prompt,
                                )?;
                            }
                            KeyCode::Down => {
                                self.handle_history_down(
                                    &mut line,
                                    &mut cursor,
                                    &mut stdout,
                                    prompt,
                                )?;
                            }
                            KeyCode::Left => {
                                if cursor > 0 {
                                    cursor -= 1;
                                    move_cursor_left(&mut stdout, 1)?;
                                    stdout.flush()?;
                                }
                            }
                            KeyCode::Right => {
                                if cursor < line.len() {
                                    cursor += 1;
                                    move_cursor_right(&mut stdout, 1)?;
                                    stdout.flush()?;
                                }
                            }
                        }
                    }
                }
                byte if (0x20..=0x7e).contains(&byte) => {
                    line.insert(cursor, byte as char);
                    cursor += 1;
                    redraw_line(&mut stdout, prompt, &line, cursor)?;
                }
                _ => {}
            }
        }
    }

    fn handle_history_up(
        &mut self,
        line: &mut String,
        cursor: &mut usize,
        stdout: &mut io::Stdout,
        prompt: &str,
    ) -> io::Result<()> {
        if self.history_index == self.history.len() {
            self.saved_line = line.clone();
        }
        if self.history_index > 0 {
            self.history_index -= 1;
            *line = self.history[self.history_index].clone();
            *cursor = line.len();
            redraw_line(stdout, prompt, line, *cursor)?;
        }
        Ok(())
    }

    fn handle_history_down(
        &mut self,
        line: &mut String,
        cursor: &mut usize,
        stdout: &mut io::Stdout,
        prompt: &str,
    ) -> io::Result<()> {
        if self.history_index < self.history.len() {
            self.history_index += 1;
            *line = if self.history_index == self.history.len() {
                self.saved_line.clone()
            } else {
                self.history[self.history_index].clone()
            };
            *cursor = line.len();
            redraw_line(stdout, prompt, line, *cursor)?;
        }
        Ok(())
    }
}

fn handle_completions(
    stdout: &mut io::Stdout,
    prompt: &str,
    line: &mut String,
    cursor: &mut usize,
    completions: Vec<CompletionEntry>,
) -> io::Result<()> {
    match completions.as_slice() {
        [] => Ok(()),
        [entry] => {
            *line = apply_completion(line, *cursor, entry);
            *cursor = line.len();
            redraw_line(stdout, prompt, line, *cursor)
        }
        entries => {
            let prefix = common_prefix(entries);
            let span = token_span(line, *cursor);
            let current = &line[span.start..(*cursor).min(span.end)];

            if prefix.len() > current.len() && prefix.starts_with(current) {
                let mut new_line = String::with_capacity(line.len() + prefix.len());
                new_line.push_str(&line[..span.start]);
                new_line.push_str(&prefix);
                new_line.push_str(&line[span.end..]);
                *line = new_line;
                *cursor = span.start + prefix.len();
                redraw_line(stdout, prompt, line, *cursor)
            } else {
                write_newline(stdout)?;
                print_completion_grid(stdout, entries)?;
                redraw_line(stdout, prompt, line, *cursor)
            }
        }
    }
}

fn common_prefix(entries: &[CompletionEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let first = &entries[0].value;
    let mut prefix_len = first.len();
    for entry in &entries[1..] {
        prefix_len = prefix_len.min(
            first
                .chars()
                .zip(entry.value.chars())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    first.chars().take(prefix_len).collect()
}

fn redraw_line(stdout: &mut io::Stdout, prompt: &str, line: &str, cursor: usize) -> io::Result<()> {
    stdout.write_all(b"\r")?;
    stdout.write_all(prompt.as_bytes())?;
    stdout.write_all(line.as_bytes())?;
    stdout.write_all(b"\x1b[J")?;

    let back_cursor = line.len().saturating_sub(cursor);
    if back_cursor > 0 {
        move_cursor_left(stdout, back_cursor)?;
    }
    stdout.flush()
}

fn move_cursor_left(stdout: &mut io::Stdout, n: usize) -> io::Result<()> {
    if n == 0 {
        return Ok(());
    }
    write!(stdout, "\x1b[{}D", n)?;
    Ok(())
}

fn move_cursor_right(stdout: &mut io::Stdout, n: usize) -> io::Result<()> {
    if n == 0 {
        return Ok(());
    }
    write!(stdout, "\x1b[{}C", n)?;
    Ok(())
}

fn print_prompt(stdout: &mut io::Stdout, prompt: &str) -> io::Result<()> {
    stdout.write_all(prompt.as_bytes())?;
    stdout.flush()
}

fn clear_screen(stdout: &mut io::Stdout) -> io::Result<()> {
    stdout.write_all(b"\x1b[2J\x1b[H")?;
    stdout.flush()
}

fn print_completion_grid(stdout: &mut io::Stdout, entries: &[CompletionEntry]) -> io::Result<()> {
    let display_names: Vec<&str> = entries.iter().map(display_name).collect();
    let max_width = display_names
        .iter()
        .map(|name| name.len())
        .max()
        .unwrap_or(0);
    let term_width = terminal_width().max(max_width + 2);
    let col_width = (max_width + 2).max(1);
    let cols = (term_width / col_width).max(1);

    for chunk in display_names.chunks(cols) {
        for (index, name) in chunk.iter().enumerate() {
            stdout.write_all(name.as_bytes())?;
            if index + 1 != chunk.len() {
                let padding = col_width.saturating_sub(name.len());
                for _ in 0..padding {
                    stdout.write_all(b" ")?;
                }
            }
        }
        write_newline(stdout)?;
    }

    Ok(())
}

fn terminal_width() -> usize {
    let mut ws = unsafe { std::mem::zeroed::<winsize>() };
    let result = unsafe { ioctl(1, TIOCGWINSZ, &mut ws) };
    if result == 0 && ws.ws_col > 0 {
        usize::from(ws.ws_col)
    } else {
        80
    }
}

fn write_newline(stdout: &mut io::Stdout) -> io::Result<()> {
    stdout.write_all(b"\r\n")?;
    stdout.flush()
}

fn stdin_is_tty() -> bool {
    unsafe { isatty(0) == 1 }
}

fn read_byte(fd: RawFd) -> io::Result<u8> {
    let mut byte = [0u8; 1];
    let n = unsafe { read(fd, byte.as_mut_ptr() as *mut c_void, 1) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    if n == 0 {
        return Ok(4);
    }
    Ok(byte[0])
}

fn parse_escape_sequence(fd: RawFd) -> io::Result<Option<KeyCode>> {
    let first = read_byte(fd)?;
    if first != b'[' {
        return Ok(None);
    }

    let second = read_byte(fd)?;
    match second {
        b'A' => Ok(Some(KeyCode::Up)),
        b'B' => Ok(Some(KeyCode::Down)),
        b'C' => Ok(Some(KeyCode::Right)),
        b'D' => Ok(Some(KeyCode::Left)),
        _ => Ok(None),
    }
}

struct RawModeGuard {
    fd: RawFd,
    saved: termios,
}

impl RawModeGuard {
    fn new(fd: RawFd) -> io::Result<Self> {
        let mut saved = unsafe { std::mem::zeroed::<termios>() };

        if unsafe { tcgetattr(fd, &mut saved) } != 0 {
            return Err(io::Error::last_os_error());
        }

        let mut raw = saved;
        raw.c_lflag &= !(ICANON | ECHO | ISIG);
        raw.c_cc[VMIN] = 1;
        raw.c_cc[VTIME] = 0;

        if unsafe { tcsetattr(fd, TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { fd, saved })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        unsafe {
            tcsetattr(self.fd, TCSANOW, &self.saved);
        }
    }
}
