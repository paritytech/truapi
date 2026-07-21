//! Slash-command parsing and command-bar editing for the signing-host UI.

use std::fs;
use std::path::{Path, PathBuf};

use crate::LogLevel;

/// A command accepted by the signing-host command bar or `exec` mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommand {
    /// Resolve the current user's primary username through TrUAPI.
    Whoami,
    /// Answer a Polkadot Mobile pairing deeplink.
    Deeplink(String),
    /// Run a product script through the public frame endpoint.
    Script(PathBuf),
    /// Show command and keyboard help.
    Help,
    /// Clear the visible transcript.
    Clear,
    /// Copy the retained transcript to the system clipboard.
    Copy,
    /// Replace the active tracing filter with a log level.
    Log(LogLevel),
    /// Shut down the signing host.
    Quit,
}

/// Parse one slash command without invoking a shell.
pub fn parse_command(input: &str) -> Result<ShellCommand, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("enter a command starting with `/`".to_string());
    }
    if !input.starts_with('/') {
        return Err("commands start with `/`; use /help to list them".to_string());
    }

    let (name, argument) = input
        .split_once(char::is_whitespace)
        .map_or((input, ""), |(name, argument)| (name, argument.trim()));
    match name {
        "/whoami" => no_argument(name, argument, ShellCommand::Whoami),
        "/deeplink" => {
            if argument.is_empty() {
                return Err("usage: /deeplink <polkadotapp://pair?...>".to_string());
            }
            if !argument.starts_with("polkadotapp://pair?") {
                return Err("/deeplink expects a polkadotapp://pair?... URL".to_string());
            }
            Ok(ShellCommand::Deeplink(argument.to_string()))
        }
        "/script" => {
            if argument.is_empty() {
                return Err("usage: /script <path>".to_string());
            }
            Ok(ShellCommand::Script(PathBuf::from(argument)))
        }
        "/help" => no_argument(name, argument, ShellCommand::Help),
        "/clear" => no_argument(name, argument, ShellCommand::Clear),
        "/copy" => no_argument(name, argument, ShellCommand::Copy),
        "/log" => {
            if argument.is_empty() {
                return Err("usage: /log <error|warn|info|debug|trace>".to_string());
            }
            Ok(ShellCommand::Log(argument.parse::<LogLevel>()?))
        }
        "/quit" => no_argument(name, argument, ShellCommand::Quit),
        _ => Err(format!(
            "unknown command `{name}`; use /help to list commands"
        )),
    }
}

fn no_argument(name: &str, argument: &str, command: ShellCommand) -> Result<ShellCommand, String> {
    if argument.is_empty() {
        Ok(command)
    } else {
        Err(format!("{name} does not accept arguments"))
    }
}

/// One selectable completion shown above the command bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Complete input inserted into the command bar.
    pub value: String,
    /// Short description rendered beside the completion.
    pub description: &'static str,
}

const COMMANDS: &[(&str, &str)] = &[
    ("/whoami", "show the current TrUAPI user id"),
    ("/deeplink ", "answer a Polkadot Mobile pairing URL"),
    ("/script ", "run a JS/TS product script"),
    ("/log ", "set error, warn, info, debug, or trace"),
    ("/help", "show commands and keyboard shortcuts"),
    ("/clear", "clear the visible transcript"),
    ("/copy", "copy the transcript to the clipboard"),
    ("/quit", "shut down the signing host"),
];

/// Compute slash-command or `/script` filesystem completions for `input`.
pub fn completions(input: &str) -> Vec<Completion> {
    if let Some(path) = input.strip_prefix("/script ") {
        return path_completions(path);
    }
    if !input.starts_with('/') || input.contains(char::is_whitespace) {
        return Vec::new();
    }
    COMMANDS
        .iter()
        .filter(|(command, _)| command.trim_end().starts_with(input))
        .map(|(command, description)| Completion {
            value: (*command).to_string(),
            description,
        })
        .collect()
}

fn path_completions(input: &str) -> Vec<Completion> {
    let path = Path::new(input);
    let ends_with_separator = input.ends_with(std::path::MAIN_SEPARATOR);
    let (directory, prefix) = if ends_with_separator {
        (path, "")
    } else {
        (
            path.parent().unwrap_or_else(|| Path::new(".")),
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(""),
        )
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let displayed_parent = if ends_with_separator {
        input.to_string()
    } else {
        input.strip_suffix(prefix).unwrap_or_default().to_string()
    };
    let mut matches = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with(prefix) {
                return None;
            }
            let suffix = if entry.file_type().ok()?.is_dir() {
                "/"
            } else {
                ""
            };
            Some(Completion {
                value: format!("/script {displayed_parent}{name}{suffix}"),
                description: "filesystem path",
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.value.cmp(&right.value));
    matches.truncate(8);
    matches
}

/// Editable command input with completion selection and in-memory history.
#[derive(Debug, Default)]
pub struct CommandEditor {
    chars: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    history_draft: String,
    completion_index: usize,
    completions_dismissed: bool,
}

impl CommandEditor {
    /// Return the current command text.
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// Return the character-indexed cursor position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replace the command text and place the cursor at its end.
    pub fn set_text(&mut self, value: impl Into<String>) {
        self.chars = value.into().chars().collect();
        self.cursor = self.chars.len();
        self.edited();
    }

    /// Insert one character at the cursor.
    pub fn insert(&mut self, value: char) {
        self.chars.insert(self.cursor, value);
        self.cursor += 1;
        self.edited();
    }

    /// Remove the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
            self.edited();
        }
    }

    /// Remove the character under the cursor.
    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
            self.edited();
        }
    }

    /// Move the cursor one character left.
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move the cursor one character right.
    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    /// Move the cursor to the start of the input.
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the input.
    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// Clear the current command without adding it to history.
    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
        self.edited();
    }

    /// Return currently visible completions.
    pub fn completions(&self) -> Vec<Completion> {
        if self.completions_dismissed {
            Vec::new()
        } else {
            completions(&self.text())
        }
    }

    /// Return the selected completion index, clamped to visible results.
    pub fn completion_index(&self) -> usize {
        self.completion_index
            .min(self.completions().len().saturating_sub(1))
    }

    /// Move to the previous completion, or older history when none is shown.
    pub fn up(&mut self) {
        let completions = self.completions();
        if !completions.is_empty() {
            self.completion_index = self
                .completion_index()
                .checked_sub(1)
                .unwrap_or(completions.len() - 1);
            return;
        }
        self.older_history();
    }

    /// Move to the next completion, or newer history when none is shown.
    pub fn down(&mut self) {
        let completions = self.completions();
        if !completions.is_empty() {
            self.completion_index = (self.completion_index() + 1) % completions.len();
            return;
        }
        self.newer_history();
    }

    /// Insert the selected completion, returning whether one existed.
    pub fn accept_completion(&mut self) -> bool {
        let completions = self.completions();
        let Some(completion) = completions.get(self.completion_index()) else {
            return false;
        };
        self.set_text(completion.value.clone());
        true
    }

    /// Hide completions until the command text changes.
    pub fn dismiss_completions(&mut self) {
        self.completions_dismissed = true;
    }

    /// Submit and remember the current input, clearing the editor.
    pub fn submit(&mut self) -> String {
        let value = self.text();
        if !value.trim().is_empty() && self.history.last() != Some(&value) {
            self.history.push(value.clone());
        }
        self.chars.clear();
        self.cursor = 0;
        self.history_index = None;
        self.history_draft.clear();
        self.edited();
        value
    }

    fn edited(&mut self) {
        self.completion_index = 0;
        self.completions_dismissed = false;
        self.history_index = None;
    }

    fn older_history(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = match self.history_index {
            Some(index) => index.saturating_sub(1),
            None => {
                self.history_draft = self.text();
                self.history.len() - 1
            }
        };
        self.history_index = Some(index);
        self.chars = self.history[index].chars().collect();
        self.cursor = self.chars.len();
    }

    fn newer_history(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 < self.history.len() {
            let next = index + 1;
            self.history_index = Some(next);
            self.chars = self.history[next].chars().collect();
        } else {
            self.history_index = None;
            self.chars = self.history_draft.chars().collect();
        }
        self.cursor = self.chars.len();
    }
}

/// Parse a confirmation answer, returning `None` for invalid input.
pub fn parse_approval(input: &str) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Some(true),
        "n" | "no" => Some(false),
        _ => None,
    }
}

/// Text displayed by `/help` in either presentation mode.
pub const HELP_TEXT: &str = "\
/whoami                 show the current TrUAPI user id
/deeplink <url>         answer a Polkadot Mobile pairing URL
/script <path>          run a JS/TS product script
/log <level>            set error, warn, info, debug, or trace
/help                   show this help
/clear                  clear the visible transcript
/copy                   copy the transcript to the clipboard
/quit                   shut down the signing host

Keys: Up/Down completion or history, Tab complete, Ctrl-U/Ctrl-D scroll,
Esc close completion or reject approval, Ctrl-C clear/cancel/quit";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_operational_commands() {
        assert_eq!(parse_command("/whoami"), Ok(ShellCommand::Whoami));
        assert_eq!(
            parse_command("/deeplink polkadotapp://pair?handshake=01"),
            Ok(ShellCommand::Deeplink(
                "polkadotapp://pair?handshake=01".to_string()
            ))
        );
        assert_eq!(
            parse_command("/script scripts/my smoke.ts"),
            Ok(ShellCommand::Script(PathBuf::from("scripts/my smoke.ts")))
        );
        assert_eq!(
            parse_command("/log trace"),
            Ok(ShellCommand::Log(LogLevel::Trace))
        );
        assert_eq!(parse_command("/copy"), Ok(ShellCommand::Copy));
    }

    #[test]
    fn rejects_bare_and_malformed_commands() {
        assert!(
            parse_command("whoami")
                .unwrap_err()
                .contains("start with `/`")
        );
        assert!(parse_command("/whoami now").is_err());
        assert!(parse_command("/copy now").is_err());
        assert!(parse_command("/deeplink https://example.com").is_err());
        assert!(parse_command("/log noisy").is_err());
    }

    #[test]
    fn completion_selection_and_history_have_distinct_arrow_behavior() {
        let mut editor = CommandEditor::default();
        editor.set_text("/");
        let first = editor.completions()[0].value.clone();
        editor.down();
        assert_ne!(editor.completions()[editor.completion_index()].value, first);

        editor.dismiss_completions();
        editor.set_text("/whoami");
        editor.submit();
        editor.set_text("draft");
        editor.dismiss_completions();
        editor.up();
        assert_eq!(editor.text(), "/whoami");
        editor.down();
        assert_eq!(editor.text(), "draft");
    }

    #[test]
    fn editor_handles_unicode_at_character_boundaries() {
        let mut editor = CommandEditor::default();
        editor.set_text("/script café.ts");
        editor.left();
        editor.left();
        editor.left();
        editor.backspace();
        assert_eq!(editor.text(), "/script caf.ts");
    }

    #[test]
    fn approval_parser_is_trimmed_and_case_insensitive() {
        assert_eq!(parse_approval(" YES "), Some(true));
        assert_eq!(parse_approval("n"), Some(false));
        assert_eq!(parse_approval("sure"), None);
    }

    #[test]
    fn script_completion_lists_matching_filesystem_paths() {
        let prefix = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("signing_s");
        let matches = completions(&format!("/script {}", prefix.display()));

        assert!(
            matches
                .iter()
                .any(|completion| completion.value.ends_with("signing_shell.rs"))
        );
    }
}
