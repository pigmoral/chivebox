use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::completion::{complete_path, display_name, token_span};
use super::shell::{
    ensure_default_path, execute_line, expand_word_fields_for_test, remove_env_var, set_env_var,
    set_env_var_os, tokenize_for_test, ShellState, DEFAULT_PATH,
};

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
    assert_eq!(tokenize_for_test("echo \"a b\" && cat < input").unwrap(), 6);
}

#[test]
fn parses_list_pipeline_and_redirection() {
    assert_eq!(
        tokenize_for_test("FOO=bar echo hi | cat >> out && pwd").unwrap(),
        9
    );
}

#[test]
fn expands_unquoted_variable_with_field_splitting() {
    set_env_var("CHIVE_SPLIT", "1 2");
    let state = ShellState::new(PathBuf::from("."));

    assert_eq!(
        expand_word_fields_for_test("a${CHIVE_SPLIT}b", &state).unwrap(),
        ["a1", "2b"]
    );

    remove_env_var("CHIVE_SPLIT");
}

#[test]
fn expands_quoted_variable_without_field_splitting() {
    set_env_var("CHIVE_QUOTED", "1 2");
    let state = ShellState::new(PathBuf::from("."));

    assert_eq!(
        expand_word_fields_for_test("\"$CHIVE_QUOTED\"", &state).unwrap(),
        ["1 2"]
    );

    remove_env_var("CHIVE_QUOTED");
}

#[test]
fn executes_and_or_lists() {
    let mut state = ShellState::new(env::current_dir().unwrap());
    let status = execute_line("false && pwd || true", &mut state).unwrap();
    assert_eq!(status, 0);
}

#[test]
fn ctrl_c_style_signal_status_maps_to_130() {
    let mut state = ShellState::new(env::current_dir().unwrap());
    let status = execute_line("/bin/sh -c 'kill -INT $$'", &mut state).unwrap();
    assert_eq!(status, 130);
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
fn default_path_is_installed_when_missing() {
    let previous = env::var_os("PATH");
    remove_env_var("PATH");

    ensure_default_path();

    assert_eq!(env::var("PATH").unwrap(), DEFAULT_PATH);

    match previous {
        Some(value) => set_env_var_os("PATH", &value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn completes_nested_absolute_paths_without_dropping_parent_dir() {
    let matches = complete_path("/proc/cp", Path::new("/"), false);
    assert!(matches.iter().any(|entry| entry.value == "/proc/cpuinfo"));
}

#[test]
fn completes_relative_paths_with_directory_prefix() {
    let temp = TempDir::new();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/main.rs"), b"").unwrap();

    let matches = complete_path("src/ma", temp.path(), false);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].value, "src/main.rs");
}

#[test]
fn completes_directory_entries_with_trailing_slash() {
    let temp = TempDir::new();
    fs::create_dir_all(temp.path().join("nested/child")).unwrap();

    let matches = complete_path("nested/", temp.path(), false);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].value, "nested/child/");
}

#[test]
fn completion_display_name_uses_basename_only() {
    let file = complete_path("/bin/ca", Path::new("/"), false)
        .into_iter()
        .find(|entry| entry.value == "/bin/cat")
        .unwrap();
    assert_eq!(display_name(&file), "cat");

    let dir = super::completion::CompletionEntry {
        value: "nested/child/".to_string(),
        is_dir: true,
    };
    assert_eq!(display_name(&dir), "child/");
}

#[test]
fn token_span_replaces_entire_path_token() {
    let span = token_span("ls /proc/cp", 11);

    assert_eq!(span.start, 3);
    assert_eq!(span.end, 11);
}
