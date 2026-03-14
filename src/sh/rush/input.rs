use std::io::{self, Write};
use std::os::fd::RawFd;

use libc::{
    c_void, ioctl, isatty, read, tcgetattr, tcsetattr, termios, winsize, ECHO, ICANON, ISIG,
    TCSANOW, TIOCGWINSZ, VMIN, VTIME,
};

use super::completion::{apply_completion, display_name, CompletionEntry};

pub(crate) enum ReadOutcome {
    Line(String),
    Eof,
    Interrupted,
}

pub(crate) struct BasicLineEditor;

impl BasicLineEditor {
    pub(crate) fn new() -> Self {
        Self
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
        let mut stdout = io::stdout();

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
                    if line.pop().is_some() {
                        stdout.write_all(b"\x08 \x08")?;
                        stdout.flush()?;
                    }
                }
                b'\t' => {
                    let completions = completer(&line, line.len());
                    handle_completions(&mut stdout, prompt, &mut line, completions)?;
                }
                12 => {
                    clear_screen(&mut stdout)?;
                    print_prompt(&mut stdout, prompt)?;
                    stdout.write_all(line.as_bytes())?;
                    stdout.flush()?;
                }
                0x1b => consume_escape_sequence(0)?,
                byte if (0x20..=0x7e).contains(&byte) => {
                    let ch = byte as char;
                    line.push(ch);
                    stdout.write_all(&[byte])?;
                    stdout.flush()?;
                }
                _ => {}
            }
        }
    }
}

fn handle_completions(
    stdout: &mut io::Stdout,
    prompt: &str,
    line: &mut String,
    completions: Vec<CompletionEntry>,
) -> io::Result<()> {
    match completions.as_slice() {
        [] => Ok(()),
        [entry] => {
            let old_display_len = line.len();
            *line = apply_completion(line, line.len(), entry);
            redraw_line(stdout, prompt, line, old_display_len)
        }
        entries => {
            write_newline(stdout)?;
            print_completion_grid(stdout, entries)?;
            print_prompt(stdout, prompt)?;
            stdout.write_all(line.as_bytes())?;
            stdout.flush()
        }
    }
}

fn redraw_line(
    stdout: &mut io::Stdout,
    prompt: &str,
    line: &str,
    old_line_len: usize,
) -> io::Result<()> {
    stdout.write_all(b"\r")?;
    stdout.write_all(prompt.as_bytes())?;
    stdout.write_all(line.as_bytes())?;
    if old_line_len > line.len() {
        for _ in 0..(old_line_len - line.len()) {
            stdout.write_all(b" ")?;
        }
        stdout.write_all(b"\r")?;
        stdout.write_all(prompt.as_bytes())?;
        stdout.write_all(line.as_bytes())?;
    }
    stdout.flush()
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
    // SAFETY: ioctl fills the provided winsize for a tty-like fd.
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
    // SAFETY: isatty reads fd metadata only.
    unsafe { isatty(0) == 1 }
}

fn read_byte(fd: RawFd) -> io::Result<u8> {
    let mut byte = [0u8; 1];
    // SAFETY: read writes exactly one byte into the provided valid buffer.
    let n = unsafe { read(fd, byte.as_mut_ptr() as *mut c_void, 1) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    if n == 0 {
        return Ok(4);
    }
    Ok(byte[0])
}

fn consume_escape_sequence(fd: RawFd) -> io::Result<()> {
    let first = read_byte(fd)?;
    if first != b'[' {
        return Ok(());
    }

    let _ = read_byte(fd)?;
    Ok(())
}

struct RawModeGuard {
    fd: RawFd,
    saved: termios,
}

impl RawModeGuard {
    fn new(fd: RawFd) -> io::Result<Self> {
        let mut saved = unsafe { std::mem::zeroed::<termios>() };

        // SAFETY: tcgetattr/tcsetattr operate on a valid tty file descriptor.
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
        // SAFETY: restores the previously captured termios state.
        unsafe {
            tcsetattr(self.fd, TCSANOW, &self.saved);
        }
    }
}
