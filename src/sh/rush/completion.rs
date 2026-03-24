use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::applets;

use super::shell::BUILTIN_NAMES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TokenSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CompletionEntry {
    pub(crate) value: String,
    pub(crate) is_dir: bool,
}

struct PathRequest {
    dir: PathBuf,
    name_prefix: String,
    value_prefix: String,
}

pub(crate) fn token_span(line: &str, pos: usize) -> TokenSpan {
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

pub(crate) fn apply_completion(line: &str, pos: usize, entry: &CompletionEntry) -> String {
    let span = token_span(line, pos);
    let mut updated = String::with_capacity(line.len() + entry.value.len() + 1);
    updated.push_str(&line[..span.start]);
    updated.push_str(&entry.value);
    if !entry.is_dir {
        updated.push(' ');
    }
    updated.push_str(&line[span.end..]);
    updated
}

pub(crate) fn display_name(entry: &CompletionEntry) -> &str {
    let trimmed = entry.value.strip_suffix('/').unwrap_or(&entry.value);
    let name = trimmed.rsplit('/').next().unwrap_or(trimmed);

    if entry.is_dir && entry.value.ends_with('/') {
        let start = entry.value.len() - name.len() - 1;
        &entry.value[start..]
    } else {
        name
    }
}

pub(crate) fn complete(line: &str, pos: usize, cwd: &Path) -> Vec<CompletionEntry> {
    let span = token_span(line, pos);
    let token = &line[span.start..pos.min(span.end)];

    if is_command_position(line, &span)
        && !token.starts_with('/')
        && !token.starts_with("./")
        && !token.starts_with("../")
        && !token.starts_with("~/")
    {
        complete_command(token)
    } else {
        let only_dirs = matches!(current_command(line, &span).as_deref(), Some("cd"));
        complete_path(token, cwd, only_dirs)
    }
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
            dir: home,
            name_prefix: String::new(),
            value_prefix: "~/".to_string(),
        };
    }

    if let Some(rest) = token.strip_prefix("~/") {
        let home = PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/".to_string()));
        return path_request_with_base(rest, &home, "~/");
    }

    if token.starts_with('/') {
        return absolute_path_request(token);
    }

    path_request_with_base(token, cwd, "")
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

    if token.ends_with('/') {
        return PathRequest {
            dir: base.join(token.trim_start_matches('/')),
            name_prefix: String::new(),
            value_prefix: format!("{display_prefix}{token}"),
        };
    }

    if let Some((dir_part, name_prefix)) = token.rsplit_once('/') {
        return PathRequest {
            dir: base.join(dir_part),
            name_prefix: name_prefix.to_string(),
            value_prefix: if dir_part.is_empty() {
                display_prefix.to_string()
            } else {
                format!("{display_prefix}{dir_part}/")
            },
        };
    }

    PathRequest {
        dir: base.to_path_buf(),
        name_prefix: token.to_string(),
        value_prefix: display_prefix.to_string(),
    }
}

pub(crate) fn complete_path(token: &str, cwd: &Path, only_dirs: bool) -> Vec<CompletionEntry> {
    let request = path_request(token, cwd);
    let mut matches = Vec::new();

    if let Ok(entries) = fs::read_dir(&request.dir) {
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };

            if !request.name_prefix.is_empty() && !name.starts_with(&request.name_prefix) {
                continue;
            }

            let is_symlink = fs::symlink_metadata(entry.path())
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);

            let is_dir = if is_symlink {
                fs::metadata(entry.path())
                    .map(|m| m.is_dir())
                    .unwrap_or(false)
            } else {
                entry.metadata().map(|m| m.is_dir()).unwrap_or(false)
            };

            if only_dirs && !is_dir {
                continue;
            }

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
