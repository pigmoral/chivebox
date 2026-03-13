use std::collections::BTreeSet;
use std::env;
use std::ffi::{CString, OsString};
use std::fs::{self, OpenOptions};
use std::os::fd::{IntoRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use libc::{WEXITSTATUS, WIFEXITED, c_int, dup, dup2, execvp, fork, pipe, waitpid};

use reedline::{
    ColumnarMenu, Completer, DefaultPrompt, EditCommand, Emacs, KeyCode, KeyModifiers, MenuBuilder,
    Reedline, ReedlineEvent, ReedlineMenu, Suggestion, default_emacs_keybindings,
};

use crate::applets;

const BUILTIN_NAMES: &[&str] = &["cd", "exit", "export", "pwd", "unset"];

pub fn run_shell() -> i32 {
    let cwd = env::current_dir().unwrap_or_else(|_| Path::new("/").to_path_buf());
    let mut state = ShellState::new(cwd);

    let mut key_bindings = default_emacs_keybindings();
    key_bindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
            ReedlineEvent::Edit(vec![EditCommand::Complete]),
        ]),
    );
    key_bindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );

    let edit_mode = Emacs::new(key_bindings);
    let completion_menu = Box::new(
        ColumnarMenu::default()
            .with_name("completion_menu")
            .with_columns(10),
    );

    let mut rl = Reedline::create()
        .with_completer(Box::new(ShellCompleter::new()))
        .with_quick_completions(true)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(Box::new(edit_mode));

    loop {
        let prompt = DefaultPrompt::default();
        match rl.read_line(&prompt) {
            Ok(reedline::Signal::Success(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let status = match execute_line(line, &mut state) {
                    Ok(status) => status,
                    Err(err) => {
                        eprintln!("sh: {err}");
                        state.last_status = 2;
                        2
                    }
                };

                state.last_status = status;

                if let Some(code) = state.exit_code {
                    return code;
                }
            }
            Ok(reedline::Signal::CtrlC) => {
                println!("^C");
                state.last_status = 130;
            }
            Ok(reedline::Signal::CtrlD) => {
                println!("exit");
                return state.last_status;
            }
            Err(err) => {
                eprintln!("readline error: {err:?}");
                return 1;
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ShellState {
    cwd: PathBuf,
    last_status: i32,
    exit_code: Option<i32>,
}

impl ShellState {
    fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            last_status: 0,
            exit_code: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunCondition {
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

fn execute_line(line: &str, state: &mut ShellState) -> Result<i32, String> {
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
    {
        if *existing_quoted == quoted {
            existing.push_str(&value);
            return;
        }
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
            let digit_len = first.len_utf8();
            return (
                1 + digit_len,
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
            Some(token) => {
                return Err(format!("unexpected token after pipeline: {token:?}"));
            }
            None => break,
        }
    }

    Ok(CommandLine { items })
}

fn parse_pipeline(tokens: &[Token], pos: &mut usize) -> Result<Pipeline, String> {
    let mut commands = Vec::new();
    commands.push(parse_simple_command(tokens, pos)?);

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
                if words.is_empty() {
                    if let Some(assignment) = parse_assignment_word(word)? {
                        assignments.push(assignment);
                        *pos += 1;
                        continue;
                    }
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

    let value_raw = &word.raw[eq_index + 1..];
    let value = parse_word_fragment(value_raw)?;

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
        let is_last = index + 1 == pipeline.commands.len();
        let pipe_pair = if is_last { None } else { Some(create_pipe()?) };

        // SAFETY: fork creates a child process; we only do async-signal-safe work before exec/_exit.
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
            let status = execute_child_command(command, &mut child_state).unwrap_or(1);
            // SAFETY: _exit terminates the child process without running unwinding code.
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
        let status = wait_for_pid(pid)?;
        last_status = status;
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
    // SAFETY: fork creates a child process; child exits via exec/_exit.
    let pid = unsafe { fork() };
    if pid < 0 {
        return Err("fork failed".to_string());
    }

    if pid == 0 {
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

        let status = execute_child_command(&command, &mut child_state).unwrap_or(1);
        // SAFETY: _exit terminates the child process without touching shared state.
        unsafe { libc::_exit(status) };
    }

    wait_for_pid(pid)
}

fn exec_command(argv: &[String]) -> Result<i32, String> {
    if argv.is_empty() {
        return Ok(0);
    }

    if !argv[0].contains('/') && applets::find_applet(&argv[0]).is_some() {
        let exe = env::current_exe().map_err(|err| format!("failed to locate chivebox: {err}"))?;
        let exe_bytes = exe.as_os_str().as_bytes();
        let exe_c = CString::new(exe_bytes).map_err(|_| "invalid executable path".to_string())?;

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

fn exec_cstrings(cmd: &CString, args: &[CString]) {
    let mut ptrs: Vec<*const libc::c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
    ptrs.push(std::ptr::null());

    // SAFETY: argv pointers remain alive for the duration of execvp call.
    unsafe {
        execvp(cmd.as_ptr(), ptrs.as_ptr());
    }
}

fn wait_for_pid(pid: libc::pid_t) -> Result<i32, String> {
    let mut status: c_int = 0;
    // SAFETY: pid is returned from fork; status points to valid memory.
    let wait_result = unsafe { waitpid(pid, &mut status, 0) };
    if wait_result < 0 {
        return Err("waitpid failed".to_string());
    }

    if WIFEXITED(status) {
        Ok(WEXITSTATUS(status))
    } else {
        Ok(1)
    }
}

fn create_pipe() -> Result<(RawFd, RawFd), String> {
    let mut fds = [0; 2];
    // SAFETY: pipe expects space for two file descriptors.
    if unsafe { pipe(fds.as_mut_ptr()) } < 0 {
        return Err("pipe failed".to_string());
    }
    Ok((fds[0], fds[1]))
}

fn dup_to(from: RawFd, to: RawFd) -> Result<(), String> {
    // SAFETY: dup2 duplicates an open file descriptor onto another descriptor number.
    if unsafe { dup2(from, to) } < 0 {
        return Err("dup2 failed".to_string());
    }
    Ok(())
}

fn close_fd(fd: RawFd) {
    // SAFETY: closing an fd is safe; errors are ignored during cleanup paths.
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
        // SAFETY: dup duplicates an existing fd so we can restore it later.
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
        RedirectionKind::Input => OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(|err| format!("{}: {err}", path.display()))?,
        RedirectionKind::Output => OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|err| format!("{}: {err}", path.display()))?,
        RedirectionKind::Append => OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|err| format!("{}: {err}", path.display()))?,
    };

    Ok(file.into_raw_fd())
}

fn set_env_var(name: &str, value: &str) {
    // SAFETY: chivebox shell is single-threaded; mutating process env is acceptable here.
    unsafe {
        env::set_var(name, value);
    }
}

fn set_env_var_os(name: &str, value: &OsString) {
    // SAFETY: chivebox shell is single-threaded; mutating process env is acceptable here.
    unsafe {
        env::set_var(name, value);
    }
}

fn remove_env_var(name: &str) {
    // SAFETY: chivebox shell is single-threaded; mutating process env is acceptable here.
    unsafe {
        env::remove_var(name);
    }
}

struct ShellCompleter;

struct TokenSpan {
    start: usize,
    end: usize,
}

struct PathRequest {
    dir: PathBuf,
    name_prefix: String,
    value_prefix: String,
}

struct CompletionEntry {
    value: String,
    is_dir: bool,
}

impl ShellCompleter {
    fn new() -> Self {
        Self
    }

    fn token_span(line: &str, pos: usize) -> TokenSpan {
        let pos = pos.min(line.len());
        let bytes = line.as_bytes();

        let mut start = pos;
        while start > 0 && !bytes[start - 1].is_ascii_whitespace() {
            start -= 1;
        }

        let mut end = pos;
        while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
            end += 1;
        }

        TokenSpan { start, end }
    }

    fn is_command_position(line: &str, span: &TokenSpan) -> bool {
        line[..span.start].trim().is_empty()
    }

    fn current_command(line: &str, span: &TokenSpan) -> Option<String> {
        line[..span.start]
            .split_whitespace()
            .next()
            .map(str::to_string)
    }

    fn path_request(token: &str, cwd: &Path) -> PathRequest {
        if token == "~" {
            let home = PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/".to_string()));
            return PathRequest {
                dir: home.clone(),
                name_prefix: String::new(),
                value_prefix: "~/".to_string(),
            };
        }

        if let Some(rest) = token.strip_prefix("~/") {
            let home = PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/".to_string()));
            return Self::path_request_with_base(rest, &home, "~/");
        }

        if token.starts_with('/') {
            return Self::absolute_path_request(token);
        }

        Self::path_request_with_base(token, cwd, "")
    }

    fn absolute_path_request(token: &str) -> PathRequest {
        if token == "/" {
            return PathRequest {
                dir: PathBuf::from("/"),
                name_prefix: String::new(),
                value_prefix: "/".to_string(),
            };
        }

        if token.ends_with('/') {
            return PathRequest {
                dir: PathBuf::from(token),
                name_prefix: String::new(),
                value_prefix: token.to_string(),
            };
        }

        if let Some((dir_part, name_prefix)) = token.rsplit_once('/') {
            let dir = if dir_part.is_empty() {
                PathBuf::from("/")
            } else {
                PathBuf::from(dir_part)
            };

            let value_prefix = if dir_part.is_empty() {
                "/".to_string()
            } else {
                format!("{dir_part}/")
            };

            return PathRequest {
                dir,
                name_prefix: name_prefix.to_string(),
                value_prefix,
            };
        }

        PathRequest {
            dir: PathBuf::from("/"),
            name_prefix: token.trim_start_matches('/').to_string(),
            value_prefix: "/".to_string(),
        }
    }

    fn path_request_with_base(token: &str, base: &Path, display_prefix: &str) -> PathRequest {
        if token.is_empty() {
            return PathRequest {
                dir: base.to_path_buf(),
                name_prefix: String::new(),
                value_prefix: display_prefix.to_string(),
            };
        }

        if token == "/" {
            return PathRequest {
                dir: PathBuf::from("/"),
                name_prefix: String::new(),
                value_prefix: "/".to_string(),
            };
        }

        if token.ends_with('/') {
            return PathRequest {
                dir: base.join(token.trim_start_matches('/')),
                name_prefix: String::new(),
                value_prefix: format!("{display_prefix}{token}"),
            };
        }

        if let Some((dir_part, name_prefix)) = token.rsplit_once('/') {
            let dir = if token.starts_with('/') {
                PathBuf::from(format!("/{dir_part}"))
            } else {
                base.join(dir_part)
            };

            let prefix = if dir_part.is_empty() {
                display_prefix.to_string()
            } else {
                format!("{display_prefix}{dir_part}/")
            };

            return PathRequest {
                dir,
                name_prefix: name_prefix.to_string(),
                value_prefix: prefix,
            };
        }

        PathRequest {
            dir: base.to_path_buf(),
            name_prefix: token.to_string(),
            value_prefix: display_prefix.to_string(),
        }
    }

    fn complete_path(token: &str, cwd: &Path, only_dirs: bool) -> Vec<CompletionEntry> {
        let request = Self::path_request(token, cwd);
        let mut matches = Vec::new();

        if let Ok(entries) = fs::read_dir(&request.dir) {
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };

                if only_dirs && !file_type.is_dir() {
                    continue;
                }

                let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                    continue;
                };

                if !request.name_prefix.is_empty() && !name.starts_with(&request.name_prefix) {
                    continue;
                }

                let is_dir = file_type.is_dir();
                let mut value = format!("{}{}", request.value_prefix, name);
                if is_dir {
                    value.push('/');
                }

                matches.push(CompletionEntry { value, is_dir });
            }
        }

        matches.sort_by(|a, b| a.value.cmp(&b.value));
        matches
    }

    fn complete_command(token: &str) -> Vec<CompletionEntry> {
        let mut names = BTreeSet::new();
        for builtin in BUILTIN_NAMES {
            names.insert((*builtin).to_string());
        }
        for applet in applets::list_applets() {
            names.insert(applet.name.to_string());
        }

        if let Some(path_var) = env::var_os("PATH") {
            for dir in env::split_paths(&path_var) {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let Ok(file_type) = entry.file_type() else {
                            continue;
                        };
                        if !file_type.is_file() {
                            continue;
                        }
                        if let Some(name) = entry.file_name().to_str() {
                            names.insert(name.to_string());
                        }
                    }
                }
            }
        }

        names
            .into_iter()
            .filter(|name| name.starts_with(token))
            .map(|value| CompletionEntry {
                value,
                is_dir: false,
            })
            .collect()
    }
}

impl Completer for ShellCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let span = Self::token_span(line, pos);
        let token = &line[span.start..pos.min(span.end)];
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        let entries = if Self::is_command_position(line, &span)
            && !token.starts_with('/')
            && !token.starts_with("./")
            && !token.starts_with("../")
            && !token.starts_with("~/")
        {
            Self::complete_command(token)
        } else {
            let only_dirs = matches!(Self::current_command(line, &span).as_deref(), Some("cd"));
            Self::complete_path(token, &cwd, only_dirs)
        };

        entries
            .into_iter()
            .map(|entry| Suggestion {
                value: entry.value,
                description: None,
                style: None,
                extra: None,
                span: reedline::Span {
                    start: span.start,
                    end: span.end,
                },
                match_indices: None,
                display_override: None,
                append_whitespace: !entry.is_dir && span.end == line.len(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("chivebox-rush-test-{unique}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn tokenizes_quotes_and_operators() {
        let tokens = tokenize("echo \"a b\" && cat < input").unwrap();

        assert!(matches!(tokens[0], Token::Word(_)));
        assert!(matches!(tokens[1], Token::Word(_)));
        assert!(matches!(tokens[2], Token::AndIf));
        assert!(matches!(tokens[3], Token::Word(_)));
        assert!(matches!(
            tokens[4],
            Token::Redir {
                kind: RedirectionKind::Input,
                ..
            }
        ));
        assert!(matches!(tokens[5], Token::Word(_)));
    }

    #[test]
    fn parses_list_pipeline_and_redirection() {
        let tokens = tokenize("FOO=bar echo hi | cat >> out && pwd").unwrap();
        let parsed = parse_command_line(&tokens).unwrap();

        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].pipeline.commands.len(), 2);
        assert_eq!(parsed.items[1].run_if, RunCondition::IfSuccess);
        assert_eq!(parsed.items[0].pipeline.commands[0].assignments.len(), 1);
        assert_eq!(parsed.items[0].pipeline.commands[1].redirections.len(), 1);
    }

    #[test]
    fn expands_unquoted_variable_with_field_splitting() {
        set_env_var("CHIVE_SPLIT", "1 2");
        let state = ShellState::new(PathBuf::from("."));
        let word = parse_word_fragment("a${CHIVE_SPLIT}b").unwrap();

        assert_eq!(expand_word_fields(&word, &state), ["a1", "2b"]);

        remove_env_var("CHIVE_SPLIT");
    }

    #[test]
    fn expands_quoted_variable_without_field_splitting() {
        set_env_var("CHIVE_QUOTED", "1 2");
        let state = ShellState::new(PathBuf::from("."));
        let word = parse_word_fragment("\"$CHIVE_QUOTED\"").unwrap();

        assert_eq!(expand_word_fields(&word, &state), ["1 2"]);

        remove_env_var("CHIVE_QUOTED");
    }

    #[test]
    fn executes_and_or_lists() {
        let mut state = ShellState::new(env::current_dir().unwrap());

        let status = execute_line("false && pwd || true", &mut state).unwrap();

        assert_eq!(status, 0);
    }

    #[test]
    fn builtin_export_and_unset_work() {
        let mut state = ShellState::new(env::current_dir().unwrap());
        execute_line("export CHIVE_TEST=value", &mut state).unwrap();
        assert_eq!(env::var("CHIVE_TEST").unwrap(), "value");

        execute_line("unset CHIVE_TEST", &mut state).unwrap();
        assert!(env::var("CHIVE_TEST").is_err());
    }

    #[test]
    fn redirection_creates_output_file() {
        let temp = TempDir::new();
        let old_cwd = env::current_dir().unwrap();
        env::set_current_dir(temp.path()).unwrap();

        let mut state = ShellState::new(temp.path().to_path_buf());
        let status = execute_line("/bin/echo hello > out.txt", &mut state).unwrap();

        env::set_current_dir(old_cwd).unwrap();

        assert_eq!(status, 0);
        assert_eq!(
            fs::read_to_string(temp.path().join("out.txt")).unwrap(),
            "hello\n"
        );
    }

    #[test]
    fn completes_nested_absolute_paths_without_dropping_parent_dir() {
        let matches = ShellCompleter::complete_path("/proc/cp", Path::new("/"), false);

        assert!(matches.iter().any(|entry| entry.value == "/proc/cpuinfo"));
    }

    #[test]
    fn completes_relative_paths_with_directory_prefix() {
        let temp = TempDir::new();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/main.rs"), b"").unwrap();

        let matches = ShellCompleter::complete_path("src/ma", temp.path(), false);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].value, "src/main.rs");
    }

    #[test]
    fn completes_directory_entries_with_trailing_slash() {
        let temp = TempDir::new();
        fs::create_dir_all(temp.path().join("nested/child")).unwrap();

        let matches = ShellCompleter::complete_path("nested/", temp.path(), false);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].value, "nested/child/");
    }

    #[test]
    fn token_span_replaces_entire_path_token() {
        let span = ShellCompleter::token_span("ls /proc/cp", 11);

        assert_eq!(span.start, 3);
        assert_eq!(span.end, 11);
    }
}
