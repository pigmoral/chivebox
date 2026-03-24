use std::env;
use std::ffi::{CString, OsString};
use std::fs::OpenOptions;
use std::os::fd::{IntoRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use libc::{
    SIGINT, WEXITSTATUS, WIFEXITED, WIFSIGNALED, WTERMSIG, c_int, dup, dup2, execvp, fork, pipe,
    signal, waitpid,
};

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crate::applets;

static FOREGROUND_PID: AtomicI32 = AtomicI32::new(-1);
static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);

extern "C" fn sigint_handler(_: libc::c_int) {
    SIGINT_RECEIVED.store(true, Ordering::Relaxed);
}

pub(crate) fn setup_sigint_handler() {
    unsafe {
        libc::signal(libc::SIGINT, sigint_handler as *const () as usize);
    }
}

pub(crate) fn check_sigint() -> bool {
    SIGINT_RECEIVED.swap(false, Ordering::Relaxed)
}

#[derive(Debug)]
pub(crate) struct WaitResult {
    pub(crate) exit_code: i32,
    pub(crate) killed_by_sigint: bool,
    pub(crate) sigint_received: bool,
}

pub(crate) fn interrupt_foreground() {
    let pid = FOREGROUND_PID.load(Ordering::Relaxed);
    if pid > 0 {
        unsafe {
            libc::kill(pid, libc::SIGINT);
        }
    }
}

pub(crate) const BUILTIN_NAMES: &[&str] = &["cd", "exit", "export", "pwd", "unset"];
pub(crate) const DEFAULT_PATH: &str = "/bin:/sbin:/usr/bin:/usr/sbin";

#[derive(Clone, Debug)]
pub(crate) struct ShellState {
    pub(crate) cwd: PathBuf,
    pub(crate) last_status: i32,
    pub(crate) exit_code: Option<i32>,
}

impl ShellState {
    pub(crate) fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            last_status: 0,
            exit_code: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunCondition {
    Always,
    IfSuccess,
    IfFailure,
}

#[derive(Clone, Debug)]
struct CommandLine {
    items: Vec<ListItem>,
}

#[derive(Clone, Debug)]
struct ListItem {
    pipeline: Pipeline,
    run_if: RunCondition,
}

#[derive(Clone, Debug)]
struct Pipeline {
    commands: Vec<SimpleCommand>,
}

#[derive(Clone, Debug)]
struct SimpleCommand {
    assignments: Vec<Assignment>,
    words: Vec<Word>,
    redirections: Vec<Redirection>,
}

#[derive(Clone, Debug)]
struct Assignment {
    name: String,
    value: Word,
}

#[derive(Clone, Debug)]
struct Word {
    raw: String,
    parts: Vec<WordPart>,
}

#[derive(Clone, Debug)]
enum WordPart {
    Text { value: String, quoted: bool },
    Var { name: String, quoted: bool },
    Status { quoted: bool },
    Positional { index: usize, quoted: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RedirectionKind {
    Input,
    Output,
    Append,
}

#[derive(Clone, Debug)]
struct Redirection {
    fd: i32,
    kind: RedirectionKind,
    target: Word,
}

#[derive(Clone, Debug)]
enum Token {
    Word(Word),
    Seq,
    AndIf,
    OrIf,
    Pipe,
    Redir {
        fd: Option<i32>,
        kind: RedirectionKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuoteMode {
    None,
    Single,
    Double,
}

pub(crate) fn ensure_default_path() {
    let needs_default = match env::var_os("PATH") {
        Some(path) => path.is_empty(),
        None => true,
    };

    if needs_default {
        set_env_var("PATH", DEFAULT_PATH);
    }
}

pub(crate) fn prompt(state: &ShellState) -> String {
    let marker = if effective_uid() == 0 { '#' } else { '$' };
    format!("{}{} ", state.cwd.display(), marker)
}

pub(crate) fn continuation_prompt() -> String {
    "> ".to_string()
}

fn effective_uid() -> libc::uid_t {
    // SAFETY: geteuid reads process credentials and has no side effects.
    unsafe { libc::geteuid() }
}

pub(crate) fn is_incomplete_input(input: &str) -> bool {
    match tokenize(input) {
        Ok(tokens) => matches!(
            tokens.last(),
            Some(Token::Pipe | Token::AndIf | Token::OrIf | Token::Redir { .. })
        ),
        Err(err) => err.starts_with("unterminated "),
    }
}

pub(crate) fn execute_line(line: &str, state: &mut ShellState) -> Result<i32, String> {
    let tokens = tokenize(line)?;
    if tokens.is_empty() {
        return Ok(state.last_status);
    }

    let command_line = parse_command_line(&tokens)?;
    execute_command_line(&command_line, state)
}

fn tokenize(line: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut pos = 0;

    while pos < line.len() {
        let (ch, next) = next_char(line, pos).ok_or_else(|| "invalid utf-8 input".to_string())?;

        if ch.is_whitespace() {
            pos = next;
            continue;
        }

        if let Some((token, consumed)) = read_operator(line, pos) {
            tokens.push(token);
            pos += consumed;
            continue;
        }

        let (word, consumed) = read_word(line, pos)?;
        tokens.push(Token::Word(word));
        pos += consumed;
    }

    Ok(tokens)
}

fn next_char(input: &str, pos: usize) -> Option<(char, usize)> {
    let ch = input[pos..].chars().next()?;
    Some((ch, pos + ch.len_utf8()))
}

fn read_operator(line: &str, pos: usize) -> Option<(Token, usize)> {
    let rest = &line[pos..];

    let digit_count = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digit_count > 0 {
        let fd_text = &rest[..digit_count];
        let after = &rest[digit_count..];
        let kind = if after.starts_with(">>") {
            Some((RedirectionKind::Append, 2))
        } else if after.starts_with('>') {
            Some((RedirectionKind::Output, 1))
        } else if after.starts_with('<') {
            Some((RedirectionKind::Input, 1))
        } else {
            None
        };

        if let Some((kind, op_len)) = kind {
            let fd = fd_text.parse::<i32>().ok()?;
            return Some((Token::Redir { fd: Some(fd), kind }, digit_count + op_len));
        }
    }

    if rest.starts_with("&&") {
        return Some((Token::AndIf, 2));
    }
    if rest.starts_with("||") {
        return Some((Token::OrIf, 2));
    }
    if rest.starts_with(">>") {
        return Some((
            Token::Redir {
                fd: None,
                kind: RedirectionKind::Append,
            },
            2,
        ));
    }
    if rest.starts_with('|') {
        return Some((Token::Pipe, 1));
    }
    if rest.starts_with(';') {
        return Some((Token::Seq, 1));
    }
    if rest.starts_with('>') {
        return Some((
            Token::Redir {
                fd: None,
                kind: RedirectionKind::Output,
            },
            1,
        ));
    }
    if rest.starts_with('<') {
        return Some((
            Token::Redir {
                fd: None,
                kind: RedirectionKind::Input,
            },
            1,
        ));
    }

    None
}

fn read_word(line: &str, start: usize) -> Result<(Word, usize), String> {
    let mut raw = String::new();
    let mut parts = Vec::new();
    let mut pos = start;
    let mut mode = QuoteMode::None;
    let mut escape = false;

    while pos < line.len() {
        let (ch, next) = next_char(line, pos).ok_or_else(|| "invalid utf-8 input".to_string())?;

        if mode == QuoteMode::None && !escape && ch.is_whitespace() {
            break;
        }

        if mode == QuoteMode::None && !escape && read_operator(line, pos).is_some() {
            break;
        }

        if escape {
            raw.push(ch);
            push_text_part(&mut parts, ch.to_string(), true);
            escape = false;
            pos = next;
            continue;
        }

        match mode {
            QuoteMode::None => match ch {
                '\\' => {
                    raw.push(ch);
                    escape = true;
                    pos = next;
                }
                '\'' => {
                    raw.push(ch);
                    mode = QuoteMode::Single;
                    pos = next;
                }
                '"' => {
                    raw.push(ch);
                    mode = QuoteMode::Double;
                    pos = next;
                }
                '$' => {
                    let (consumed, part) = parse_variable(line, pos, false);
                    if let Some(part) = part {
                        raw.push_str(&line[pos..pos + consumed]);
                        parts.push(part);
                        pos += consumed;
                    } else {
                        raw.push('$');
                        push_text_part(&mut parts, "$".to_string(), false);
                        pos = next;
                    }
                }
                _ => {
                    raw.push(ch);
                    push_text_part(&mut parts, ch.to_string(), false);
                    pos = next;
                }
            },
            QuoteMode::Single => {
                raw.push(ch);
                if ch == '\'' {
                    mode = QuoteMode::None;
                } else {
                    push_text_part(&mut parts, ch.to_string(), true);
                }
                pos = next;
            }
            QuoteMode::Double => match ch {
                '\\' => {
                    raw.push(ch);
                    escape = true;
                    pos = next;
                }
                '"' => {
                    raw.push(ch);
                    mode = QuoteMode::None;
                    pos = next;
                }
                '$' => {
                    let (consumed, part) = parse_variable(line, pos, true);
                    if let Some(part) = part {
                        raw.push_str(&line[pos..pos + consumed]);
                        parts.push(part);
                        pos += consumed;
                    } else {
                        raw.push('$');
                        push_text_part(&mut parts, "$".to_string(), true);
                        pos = next;
                    }
                }
                _ => {
                    raw.push(ch);
                    push_text_part(&mut parts, ch.to_string(), true);
                    pos = next;
                }
            },
        }
    }

    if escape {
        return Err("unterminated escape sequence".to_string());
    }
    if mode == QuoteMode::Single {
        return Err("unterminated single quote".to_string());
    }
    if mode == QuoteMode::Double {
        return Err("unterminated double quote".to_string());
    }

    Ok((Word { raw, parts }, pos - start))
}

fn push_text_part(parts: &mut Vec<WordPart>, value: String, quoted: bool) {
    if value.is_empty() {
        return;
    }

    if let Some(WordPart::Text {
        value: existing,
        quoted: existing_quoted,
    }) = parts.last_mut()
        && *existing_quoted == quoted
    {
        existing.push_str(&value);
        return;
    }

    parts.push(WordPart::Text { value, quoted });
}

fn parse_variable(line: &str, start: usize, quoted: bool) -> (usize, Option<WordPart>) {
    let rest = &line[start..];
    if rest == "$" {
        return (1, None);
    }

    if rest.starts_with("$?") {
        return (2, Some(WordPart::Status { quoted }));
    }

    if let Some(after_dollar) = rest.strip_prefix("${") {
        if let Some(end) = after_dollar.find('}') {
            let name = &after_dollar[..end];
            if !name.is_empty() && is_valid_name(name) {
                return (
                    end + 3,
                    Some(WordPart::Var {
                        name: name.to_string(),
                        quoted,
                    }),
                );
            }
        }

        return (1, None);
    }

    let after = &rest[1..];
    if let Some(first) = after.chars().next() {
        if first.is_ascii_digit() {
            return (
                1 + first.len_utf8(),
                Some(WordPart::Positional {
                    index: first.to_digit(10).unwrap_or(0) as usize,
                    quoted,
                }),
            );
        }

        if is_name_start(first) {
            let mut len = first.len_utf8();
            for ch in after[first.len_utf8()..].chars() {
                if is_name_continue(ch) {
                    len += ch.len_utf8();
                } else {
                    break;
                }
            }

            return (
                1 + len,
                Some(WordPart::Var {
                    name: after[..len].to_string(),
                    quoted,
                }),
            );
        }
    }

    (1, None)
}

fn is_name_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_name_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    is_name_start(first) && chars.all(is_name_continue)
}

fn parse_command_line(tokens: &[Token]) -> Result<CommandLine, String> {
    let mut pos = 0;
    let mut items = Vec::new();
    let mut run_if = RunCondition::Always;

    while pos < tokens.len() {
        let pipeline = parse_pipeline(tokens, &mut pos)?;
        items.push(ListItem { pipeline, run_if });

        if pos >= tokens.len() {
            break;
        }

        match tokens.get(pos) {
            Some(Token::Seq) => {
                run_if = RunCondition::Always;
                pos += 1;
                if pos >= tokens.len() {
                    break;
                }
            }
            Some(Token::AndIf) => {
                run_if = RunCondition::IfSuccess;
                pos += 1;
            }
            Some(Token::OrIf) => {
                run_if = RunCondition::IfFailure;
                pos += 1;
            }
            Some(token) => return Err(format!("unexpected token after pipeline: {token:?}")),
            None => break,
        }
    }

    Ok(CommandLine { items })
}

fn parse_pipeline(tokens: &[Token], pos: &mut usize) -> Result<Pipeline, String> {
    let mut commands = vec![parse_simple_command(tokens, pos)?];

    while matches!(tokens.get(*pos), Some(Token::Pipe)) {
        *pos += 1;
        commands.push(parse_simple_command(tokens, pos)?);
    }

    Ok(Pipeline { commands })
}

fn parse_simple_command(tokens: &[Token], pos: &mut usize) -> Result<SimpleCommand, String> {
    let mut assignments = Vec::new();
    let mut words = Vec::new();
    let mut redirections = Vec::new();

    while let Some(token) = tokens.get(*pos) {
        match token {
            Token::Word(word) => {
                if words.is_empty()
                    && let Some(assignment) = parse_assignment_word(word)?
                {
                    assignments.push(assignment);
                    *pos += 1;
                    continue;
                }

                words.push(word.clone());
                *pos += 1;
            }
            Token::Redir { fd, kind } => {
                *pos += 1;
                let Some(Token::Word(target)) = tokens.get(*pos) else {
                    return Err("redirection missing target".to_string());
                };

                redirections.push(Redirection {
                    fd: fd.unwrap_or(default_fd(*kind)),
                    kind: *kind,
                    target: target.clone(),
                });
                *pos += 1;
            }
            Token::Seq | Token::AndIf | Token::OrIf | Token::Pipe => break,
        }
    }

    if assignments.is_empty() && words.is_empty() && redirections.is_empty() {
        return Err("expected command before operator".to_string());
    }

    Ok(SimpleCommand {
        assignments,
        words,
        redirections,
    })
}

fn parse_assignment_word(word: &Word) -> Result<Option<Assignment>, String> {
    let Some(eq_index) = word.raw.find('=') else {
        return Ok(None);
    };

    let name = &word.raw[..eq_index];
    if !is_valid_name(name) {
        return Ok(None);
    }

    let value = parse_word_fragment(&word.raw[eq_index + 1..])?;
    Ok(Some(Assignment {
        name: name.to_string(),
        value,
    }))
}

fn parse_word_fragment(input: &str) -> Result<Word, String> {
    if input.is_empty() {
        return Ok(Word {
            raw: String::new(),
            parts: Vec::new(),
        });
    }

    let tokens = tokenize(input)?;
    if tokens.len() != 1 {
        return Err("invalid assignment value".to_string());
    }

    match tokens.into_iter().next() {
        Some(Token::Word(word)) => Ok(word),
        _ => Err("invalid assignment value".to_string()),
    }
}

fn default_fd(kind: RedirectionKind) -> i32 {
    match kind {
        RedirectionKind::Input => 0,
        RedirectionKind::Output | RedirectionKind::Append => 1,
    }
}

fn execute_command_line(command_line: &CommandLine, state: &mut ShellState) -> Result<i32, String> {
    let mut status = state.last_status;

    for item in &command_line.items {
        let should_run = match item.run_if {
            RunCondition::Always => true,
            RunCondition::IfSuccess => status == 0,
            RunCondition::IfFailure => status != 0,
        };

        if should_run {
            status = execute_pipeline(&item.pipeline, state)?;
            state.last_status = status;
        }

        if state.exit_code.is_some() {
            break;
        }
    }

    Ok(status)
}

fn execute_pipeline(pipeline: &Pipeline, state: &mut ShellState) -> Result<i32, String> {
    if pipeline.commands.len() == 1 {
        return execute_single_command(&pipeline.commands[0], state);
    }

    let mut pids = Vec::new();
    let mut prev_read: Option<RawFd> = None;
    let mut last_status = 0;

    for (index, command) in pipeline.commands.iter().enumerate() {
        let pipe_pair = if index + 1 == pipeline.commands.len() {
            None
        } else {
            Some(create_pipe()?)
        };

        let pid = unsafe { fork() };
        if pid < 0 {
            if let Some(read_fd) = prev_read {
                close_fd(read_fd);
            }
            if let Some((read_fd, write_fd)) = pipe_pair {
                close_fd(read_fd);
                close_fd(write_fd);
            }
            return Err("fork failed".to_string());
        }

        if pid == 0 {
            reset_signal_handlers_for_child();

            if let Some(read_fd) = prev_read {
                dup_to(read_fd, 0)?;
            }
            if let Some((_, write_fd)) = pipe_pair {
                dup_to(write_fd, 1)?;
            }

            if let Some(read_fd) = prev_read {
                close_fd(read_fd);
            }
            if let Some((read_fd, write_fd)) = pipe_pair {
                close_fd(read_fd);
                close_fd(write_fd);
            }

            let mut child_state = state.clone();
            let status = match execute_child_command(command, &mut child_state) {
                Ok(status) => status,
                Err(err) => {
                    eprintln!("sh: {err}");
                    1
                }
            };
            unsafe { libc::_exit(status) };
        }

        if let Some(read_fd) = prev_read {
            close_fd(read_fd);
        }
        if let Some((read_fd, write_fd)) = pipe_pair {
            close_fd(write_fd);
            prev_read = Some(read_fd);
        } else {
            prev_read = None;
        }

        pids.push(pid);
    }

    if let Some(read_fd) = prev_read {
        close_fd(read_fd);
    }

    for pid in pids {
        let result = wait_for_pid(pid)?;
        last_status = result.exit_code;
        if result.killed_by_sigint {
            println!();
        } else if result.sigint_received {
            println!("^C");
        }
    }

    Ok(last_status)
}

fn execute_single_command(command: &SimpleCommand, state: &mut ShellState) -> Result<i32, String> {
    let assignments = expand_assignments(&command.assignments, state)?;
    let argv = expand_argv(&command.words, state);

    if argv.is_empty() {
        for (name, value) in &assignments {
            set_env_var(name, value);
        }

        let _redir_guard = apply_redirections_parent(&command.redirections, state)?;
        return Ok(0);
    }

    if let Some(kind) = BuiltinKind::from_name(&argv[0]) {
        let _assign_guard = EnvGuard::apply(&assignments);
        let _redir_guard = apply_redirections_parent(&command.redirections, state)?;
        return run_builtin(kind, &argv[1..], state);
    }

    execute_external_command(&argv, &assignments, &command.redirections, state)
}

fn execute_child_command(command: &SimpleCommand, state: &mut ShellState) -> Result<i32, String> {
    let assignments = expand_assignments(&command.assignments, state)?;
    let argv = expand_argv(&command.words, state);

    for (name, value) in &assignments {
        set_env_var(name, value);
    }

    apply_redirections_child(&command.redirections, state)?;

    if argv.is_empty() {
        return Ok(0);
    }

    if let Some(kind) = BuiltinKind::from_name(&argv[0]) {
        return run_builtin(kind, &argv[1..], state);
    }

    exec_command(&argv)
}

fn expand_assignments(
    assignments: &[Assignment],
    state: &ShellState,
) -> Result<Vec<(String, String)>, String> {
    assignments
        .iter()
        .map(|assignment| {
            Ok((
                assignment.name.clone(),
                expand_word_single(&assignment.value, state),
            ))
        })
        .collect()
}

fn expand_argv(words: &[Word], state: &ShellState) -> Vec<String> {
    let mut argv = Vec::new();
    for word in words {
        argv.extend(expand_word_fields(word, state));
    }
    argv
}

fn expand_word_fields(word: &Word, state: &ShellState) -> Vec<String> {
    let mut completed = Vec::new();
    let mut current = String::new();
    let mut preserve_empty = false;

    for part in &word.parts {
        match part {
            WordPart::Text { value, quoted } => {
                if *quoted && value.is_empty() {
                    preserve_empty = true;
                }
                current.push_str(value);
            }
            WordPart::Var { name, quoted } => {
                let value = lookup_var(name, state);
                if *quoted {
                    if value.is_empty() {
                        preserve_empty = true;
                    }
                    current.push_str(&value);
                } else {
                    apply_unquoted_expansion(&mut completed, &mut current, &value);
                }
            }
            WordPart::Status { quoted } => {
                let value = state.last_status.to_string();
                if *quoted {
                    current.push_str(&value);
                } else {
                    apply_unquoted_expansion(&mut completed, &mut current, &value);
                }
            }
            WordPart::Positional { index, quoted } => {
                let _ = index;
                if *quoted {
                    preserve_empty = true;
                }
            }
        }
    }

    if !current.is_empty() || !completed.is_empty() || preserve_empty || word.parts.is_empty() {
        completed.push(current);
    }

    completed
}

fn apply_unquoted_expansion(completed: &mut Vec<String>, current: &mut String, value: &str) {
    let fields: Vec<&str> = value.split_whitespace().collect();
    if fields.is_empty() {
        return;
    }

    if fields.len() == 1 {
        current.push_str(fields[0]);
        return;
    }

    current.push_str(fields[0]);
    completed.push(std::mem::take(current));
    for middle in &fields[1..fields.len() - 1] {
        completed.push((*middle).to_string());
    }
    *current = fields[fields.len() - 1].to_string();
}

fn expand_word_single(word: &Word, state: &ShellState) -> String {
    let mut result = String::new();
    for part in &word.parts {
        match part {
            WordPart::Text { value, .. } => result.push_str(value),
            WordPart::Var { name, .. } => result.push_str(&lookup_var(name, state)),
            WordPart::Status { .. } => result.push_str(&state.last_status.to_string()),
            WordPart::Positional { .. } => {}
        }
    }
    result
}

fn lookup_var(name: &str, _state: &ShellState) -> String {
    env::var(name).unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuiltinKind {
    Cd,
    Exit,
    Export,
    Pwd,
    Unset,
}

impl BuiltinKind {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "cd" => Some(Self::Cd),
            "exit" => Some(Self::Exit),
            "export" => Some(Self::Export),
            "pwd" => Some(Self::Pwd),
            "unset" => Some(Self::Unset),
            _ => None,
        }
    }
}

fn run_builtin(kind: BuiltinKind, args: &[String], state: &mut ShellState) -> Result<i32, String> {
    match kind {
        BuiltinKind::Cd => {
            let target = if let Some(path) = args.first() {
                expand_home(path)
            } else {
                PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/".to_string()))
            };

            env::set_current_dir(&target)
                .map_err(|err| format!("cd: {}: {err}", target.display()))?;
            state.cwd = env::current_dir().unwrap_or(target);
            set_env_var("PWD", &state.cwd.display().to_string());
            Ok(0)
        }
        BuiltinKind::Exit => {
            let code = if let Some(value) = args.first() {
                value
                    .parse::<i32>()
                    .map_err(|_| format!("exit: numeric argument required: {value}"))?
            } else {
                state.last_status
            };
            state.exit_code = Some(code);
            Ok(code)
        }
        BuiltinKind::Export => {
            for arg in args {
                if let Some((name, value)) = arg.split_once('=') {
                    if !is_valid_name(name) {
                        return Err(format!("export: invalid name: {name}"));
                    }
                    set_env_var(name, value);
                } else if !is_valid_name(arg) {
                    return Err(format!("export: invalid name: {arg}"));
                }
            }
            Ok(0)
        }
        BuiltinKind::Pwd => {
            println!("{}", state.cwd.display());
            Ok(0)
        }
        BuiltinKind::Unset => {
            for arg in args {
                if !is_valid_name(arg) {
                    return Err(format!("unset: invalid name: {arg}"));
                }
                remove_env_var(arg);
            }
            Ok(0)
        }
    }
}

fn expand_home(input: &str) -> PathBuf {
    if input == "~" {
        return PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/".to_string()));
    }

    if let Some(rest) = input.strip_prefix("~/") {
        return PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/".to_string())).join(rest);
    }

    PathBuf::from(input)
}

fn execute_external_command(
    argv: &[String],
    assignments: &[(String, String)],
    redirections: &[Redirection],
    state: &ShellState,
) -> Result<i32, String> {
    let pid = unsafe { fork() };
    if pid < 0 {
        return Err("fork failed".to_string());
    }

    if pid == 0 {
        reset_signal_handlers_for_child();

        let mut child_state = state.clone();
        let command = SimpleCommand {
            assignments: assignments
                .iter()
                .map(|(name, value)| Assignment {
                    name: name.clone(),
                    value: Word {
                        raw: value.clone(),
                        parts: vec![WordPart::Text {
                            value: value.clone(),
                            quoted: true,
                        }],
                    },
                })
                .collect(),
            words: argv
                .iter()
                .map(|arg| Word {
                    raw: arg.clone(),
                    parts: vec![WordPart::Text {
                        value: arg.clone(),
                        quoted: true,
                    }],
                })
                .collect(),
            redirections: redirections.to_vec(),
        };

        let status = match execute_child_command(&command, &mut child_state) {
            Ok(status) => status,
            Err(err) => {
                eprintln!("sh: {err}");
                1
            }
        };
        unsafe { libc::_exit(status) };
    }

    FOREGROUND_PID.store(pid, Ordering::Relaxed);
    let result = wait_for_pid(pid);
    FOREGROUND_PID.store(-1, Ordering::Relaxed);

    result.map(|r| {
        if r.killed_by_sigint {
            // Child was killed by SIGINT, print newline (like hush)
            println!();
        } else if r.sigint_received {
            // Shell received SIGINT, print ^C
            println!("^C");
        }
        r.exit_code
    })
}

fn exec_command(argv: &[String]) -> Result<i32, String> {
    if argv.is_empty() {
        return Ok(0);
    }

    if !argv[0].contains('/') && applets::find_applet(&argv[0]).is_some() {
        let exe = resolve_chivebox_exec_path()
            .ok_or_else(|| "failed to locate chivebox executable".to_string())?;
        let exe_c = CString::new(exe.as_os_str().as_bytes())
            .map_err(|_| "invalid executable path".to_string())?;

        let mut c_args = Vec::with_capacity(argv.len() + 1);
        c_args.push(exe_c.clone());
        c_args.push(CString::new(argv[0].as_str()).map_err(|_| "invalid command".to_string())?);
        for arg in &argv[1..] {
            c_args.push(CString::new(arg.as_str()).map_err(|_| "invalid argument".to_string())?);
        }

        exec_cstrings(&exe_c, &c_args);
        return Err(format!("{}: command not found", argv[0]));
    }

    let cmd = CString::new(argv[0].as_str()).map_err(|_| "invalid command".to_string())?;
    let mut c_args = Vec::with_capacity(argv.len());
    for arg in argv {
        c_args.push(CString::new(arg.as_str()).map_err(|_| "invalid argument".to_string())?);
    }

    exec_cstrings(&cmd, &c_args);
    Err(format!("{}: command not found", argv[0]))
}

fn resolve_chivebox_exec_path() -> Option<PathBuf> {
    if let Ok(path) = env::current_exe() {
        return Some(path);
    }

    for fallback in ["/bin/chivebox", "/sbin/chivebox"] {
        let path = PathBuf::from(fallback);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn exec_cstrings(cmd: &CString, args: &[CString]) {
    let mut ptrs: Vec<*const libc::c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    unsafe {
        execvp(cmd.as_ptr(), ptrs.as_ptr());
    }
}

fn wait_for_pid(pid: libc::pid_t) -> Result<WaitResult, String> {
    let mut status: c_int = 0;

    // Simple blocking wait
    let wait_result = unsafe { waitpid(pid, &mut status, 0) };
    if wait_result < 0 {
        return Err("waitpid failed".to_string());
    }

    let exit_code = if WIFEXITED(status) {
        WEXITSTATUS(status)
    } else if WIFSIGNALED(status) {
        128 + WTERMSIG(status)
    } else {
        1
    };

    // Check if child was killed by SIGINT
    let killed_by_sigint = WIFSIGNALED(status) && WTERMSIG(status) == libc::SIGINT as i32;

    // Check if SIGINT was received
    let sigint_received = SIGINT_RECEIVED.swap(false, Ordering::Relaxed);

    Ok(WaitResult {
        exit_code,
        killed_by_sigint,
        sigint_received,
    })
}

fn reset_signal_handlers_for_child() {
    // SAFETY: child process restores default SIGINT handling before exec/running command.
    unsafe {
        signal(SIGINT, libc::SIG_DFL);
    }
}

fn create_pipe() -> Result<(RawFd, RawFd), String> {
    let mut fds = [0; 2];
    if unsafe { pipe(fds.as_mut_ptr()) } < 0 {
        return Err("pipe failed".to_string());
    }
    Ok((fds[0], fds[1]))
}

fn dup_to(from: RawFd, to: RawFd) -> Result<(), String> {
    if unsafe { dup2(from, to) } < 0 {
        return Err("dup2 failed".to_string());
    }
    Ok(())
}

fn close_fd(fd: RawFd) {
    unsafe {
        libc::close(fd);
    }
}

struct EnvGuard {
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    fn apply(assignments: &[(String, String)]) -> Self {
        let mut saved = Vec::new();
        for (name, value) in assignments {
            saved.push((name.clone(), env::var_os(name)));
            set_env_var(name, value);
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in self.saved.iter().rev() {
            match value {
                Some(value) => set_env_var_os(name, value),
                None => remove_env_var(name),
            }
        }
    }
}

struct FdGuard {
    saved: Vec<(RawFd, RawFd)>,
}

impl Drop for FdGuard {
    fn drop(&mut self) {
        for (target, saved) in self.saved.iter().rev() {
            let _ = dup_to(*saved, *target);
            close_fd(*saved);
        }
    }
}

fn apply_redirections_parent(
    redirections: &[Redirection],
    state: &ShellState,
) -> Result<FdGuard, String> {
    let mut saved = Vec::new();
    for redirection in redirections {
        let opened = open_redirection(redirection, state)?;
        let backup = unsafe { dup(redirection.fd) };
        if backup < 0 {
            close_fd(opened);
            return Err("dup failed".to_string());
        }

        dup_to(opened, redirection.fd)?;
        close_fd(opened);
        saved.push((redirection.fd, backup));
    }

    Ok(FdGuard { saved })
}

fn apply_redirections_child(
    redirections: &[Redirection],
    state: &ShellState,
) -> Result<(), String> {
    for redirection in redirections {
        let opened = open_redirection(redirection, state)?;
        dup_to(opened, redirection.fd)?;
        close_fd(opened);
    }
    Ok(())
}

fn open_redirection(redirection: &Redirection, state: &ShellState) -> Result<RawFd, String> {
    let target = expand_word_single(&redirection.target, state);
    let path = expand_home(&target);

    let file = match redirection.kind {
        RedirectionKind::Input => OpenOptions::new().read(true).open(&path),
        RedirectionKind::Output => OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path),
        RedirectionKind::Append => OpenOptions::new().create(true).append(true).open(&path),
    }
    .map_err(|err| format!("{}: {err}", path.display()))?;

    Ok(file.into_raw_fd())
}

pub(crate) fn set_env_var(name: &str, value: &str) {
    unsafe { env::set_var(name, value) };
}

pub(crate) fn set_env_var_os(name: &str, value: &OsString) {
    unsafe { env::set_var(name, value) };
}

pub(crate) fn remove_env_var(name: &str) {
    unsafe { env::remove_var(name) };
}

#[cfg(test)]
pub(crate) fn expand_word_fields_for_test(
    input: &str,
    state: &ShellState,
) -> Result<Vec<String>, String> {
    let word = parse_word_fragment(input)?;
    Ok(expand_word_fields(&word, state))
}

#[cfg(test)]
pub(crate) fn tokenize_for_test(line: &str) -> Result<usize, String> {
    Ok(tokenize(line)?.len())
}
