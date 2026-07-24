//! Slash-command parsing and command-bar editing for the signing-host UI.

use std::fs;
use std::path::{Path, PathBuf};

use truapi_platform::normalize_product_identifier;

use crate::LogLevel;
use crate::sessions;

/// Operation selected through `/product`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductCommand {
    /// Print the currently selected product.
    Current,
    /// Switch to the validated, normalized product id.
    Switch(String),
}

/// Operation selected through `/session`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    /// Print the current session.
    Current,
    /// List sessions for the active network.
    List,
    /// Switch to or create the named session.
    Switch(String),
}

/// A command accepted by the signing-host command bar or `exec` mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommand {
    /// Answer a Polkadot Mobile pairing deeplink.
    Pair(String),
    /// Edit the remembered product script, or run an explicit one, through the
    /// public frame endpoint.
    Script(Option<PathBuf>),
    /// Show command and keyboard help.
    Help,
    /// Clear the visible transcript.
    Clear,
    /// Copy the retained transcript to the system clipboard.
    Copy,
    /// Start the pairing-host login flow for the selected product.
    Login,
    /// Disconnect a pairing host and discard its old pairing keypair.
    Logout,
    /// Replace the active tracing filter with a log level.
    Log(LogLevel),
    /// Inspect or switch the product used by scripts and frame connections.
    Product(ProductCommand),
    /// Inspect, list, or switch the active persistent session.
    Session(SessionCommand),
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
        "/pair" => {
            if argument.is_empty() {
                return Err("usage: /pair <polkadotapp://pair?...>".to_string());
            }
            if !argument.starts_with("polkadotapp://pair?") {
                return Err("/pair expects a polkadotapp://pair?... URL".to_string());
            }
            Ok(ShellCommand::Pair(argument.to_string()))
        }
        "/script" => {
            if argument.is_empty() {
                return Ok(ShellCommand::Script(None));
            }
            Ok(ShellCommand::Script(Some(PathBuf::from(argument))))
        }
        "/help" => no_argument(name, argument, ShellCommand::Help),
        "/clear" => no_argument(name, argument, ShellCommand::Clear),
        "/copy" => no_argument(name, argument, ShellCommand::Copy),
        "/login" => no_argument(name, argument, ShellCommand::Login),
        "/logout" => no_argument(name, argument, ShellCommand::Logout),
        "/log" => {
            if argument.is_empty() {
                return Err("usage: /log <error|warn|info|debug|trace>".to_string());
            }
            Ok(ShellCommand::Log(argument.parse::<LogLevel>()?))
        }
        "/product" => {
            if argument.is_empty() {
                return Ok(ShellCommand::Product(ProductCommand::Current));
            }
            let product_id =
                normalize_product_identifier(argument).map_err(|error| error.to_string())?;
            Ok(ShellCommand::Product(ProductCommand::Switch(product_id)))
        }
        "/session" => {
            if argument.is_empty() {
                return Ok(ShellCommand::Session(SessionCommand::Current));
            }
            if argument == "--list" {
                return Ok(ShellCommand::Session(SessionCommand::List));
            }
            sessions::validate_selectable_name(argument)?;
            Ok(ShellCommand::Session(SessionCommand::Switch(
                argument.to_string(),
            )))
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

const SIGNING_COMMANDS: &[(&str, &str)] = &[
    ("/pair ", "answer a Polkadot Mobile pairing URL"),
    ("/script", "edit the last or run an existing product script"),
    ("/log ", "set error, warn, info, debug, or trace"),
    ("/product", "show or switch the active product"),
    ("/session", "show or switch the active session"),
    ("/help", "show commands and keyboard shortcuts"),
    ("/clear", "clear the visible transcript"),
    ("/copy", "copy the transcript to the clipboard"),
    ("/quit", "shut down the signing host"),
];

const PAIRING_COMMANDS: &[(&str, &str)] = &[
    ("/script", "edit the last or run an existing product script"),
    ("/login", "pair with a signing host"),
    ("/logout", "disconnect and reset pairing keys"),
    ("/log ", "set error, warn, info, debug, or trace"),
    ("/product", "show or switch the active product"),
    ("/help", "show commands and keyboard shortcuts"),
    ("/clear", "clear the visible transcript"),
    ("/copy", "copy the transcript to the clipboard"),
    ("/quit", "shut down the pairing host"),
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CommandScope {
    PairingHost,
    #[default]
    SigningHost,
}

fn completions_for_scope(
    input: &str,
    session_names: &[String],
    scope: CommandScope,
) -> Vec<Completion> {
    if let Some(path) = input.strip_prefix("/script ") {
        return path_completions(path);
    }
    if scope == CommandScope::SigningHost
        && let Some(prefix) = input.strip_prefix("/session ")
    {
        if prefix.contains(char::is_whitespace) {
            return Vec::new();
        }
        let mut matches = session_names
            .iter()
            .filter(|name| name.starts_with(prefix))
            .map(|name| Completion {
                value: format!("/session {name}"),
                description: "existing session",
            })
            .collect::<Vec<_>>();
        if "--list".starts_with(prefix) {
            matches.insert(
                0,
                Completion {
                    value: "/session --list".to_string(),
                    description: "list sessions",
                },
            );
        }
        return matches;
    }
    if !input.starts_with('/') || input.contains(char::is_whitespace) {
        return Vec::new();
    }
    let commands = match scope {
        CommandScope::PairingHost => PAIRING_COMMANDS,
        CommandScope::SigningHost => SIGNING_COMMANDS,
    };
    commands
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
#[derive(Debug)]
pub struct CommandEditor {
    chars: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    history_draft: String,
    completion_index: usize,
    completions_dismissed: bool,
    session_names: Vec<String>,
    scope: CommandScope,
}

impl Default for CommandEditor {
    fn default() -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            completion_index: 0,
            completions_dismissed: false,
            session_names: Vec::new(),
            scope: CommandScope::SigningHost,
        }
    }
}

impl CommandEditor {
    /// Build an editor exposing only commands supported by the pairing host.
    pub fn pairing_host() -> Self {
        Self {
            scope: CommandScope::PairingHost,
            ..Self::default()
        }
    }

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

    /// Replace session names offered after `/session `.
    pub fn set_session_names(&mut self, names: Vec<String>) {
        self.session_names = names;
        self.edited();
    }

    /// Return currently visible completions.
    pub fn completions(&self) -> Vec<Completion> {
        if self.completions_dismissed {
            Vec::new()
        } else {
            completions_for_scope(&self.text(), &self.session_names, self.scope)
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
/pair <url>             answer a Polkadot Mobile pairing URL
/script                 edit and run the session's last Bun TypeScript script
/script <path>          run an existing JS/TS product script with Bun
/log <level>            set error, warn, info, debug, or trace
/product                show the current product
/product <id>           switch product and reconnect product clients
/session                show the current session and path
/session <name>         switch to or create a session
/session --list         list sessions for this network
/help                   show this help
/clear                  clear the visible transcript
/copy                   copy the transcript to the clipboard
/quit                   shut down the signing host

Keys: Up/Down completion or history, Tab complete, Ctrl-U/Ctrl-D scroll,
Esc close completion or reject approval, Ctrl-C clear/cancel/quit";

/// Help shown by the pairing-host command bar.
pub const PAIRING_HELP_TEXT: &str = "\
/script                 edit and run the last Bun TypeScript product script
/script <path>          run an existing JS/TS product script with Bun
/login                  pair with a signing host for the current product
/logout                 disconnect and reset pairing keys
/log <level>            set error, warn, info, debug, or trace
/product                show the current product
/product <id>           switch product and reconnect product clients
/help                   show this help
/clear                  clear the visible transcript
/copy                   copy the transcript to the clipboard
/quit                   shut down the pairing host

Keys: Up/Down completion or history, Tab complete, Ctrl-U/Ctrl-D scroll,
Esc close completion, Ctrl-C clear/cancel/quit";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_operational_commands() {
        assert_eq!(
            parse_command("/pair polkadotapp://pair?handshake=01"),
            Ok(ShellCommand::Pair(
                "polkadotapp://pair?handshake=01".to_string()
            ))
        );
        assert_eq!(
            parse_command("/script scripts/my smoke.ts"),
            Ok(ShellCommand::Script(Some(PathBuf::from(
                "scripts/my smoke.ts"
            ))))
        );
        assert_eq!(parse_command("/script"), Ok(ShellCommand::Script(None)));
        assert_eq!(parse_command("/login"), Ok(ShellCommand::Login));
        assert_eq!(parse_command("/logout"), Ok(ShellCommand::Logout));
        assert_eq!(
            parse_command("/log trace"),
            Ok(ShellCommand::Log(LogLevel::Trace))
        );
        assert_eq!(
            parse_command("/product"),
            Ok(ShellCommand::Product(ProductCommand::Current))
        );
        assert_eq!(
            parse_command("/product Dotli.DOT"),
            Ok(ShellCommand::Product(ProductCommand::Switch(
                "dotli.dot".to_string()
            )))
        );
        assert_eq!(parse_command("/copy"), Ok(ShellCommand::Copy));
        assert_eq!(
            parse_command("/session"),
            Ok(ShellCommand::Session(SessionCommand::Current))
        );
        assert_eq!(
            parse_command("/session alice"),
            Ok(ShellCommand::Session(SessionCommand::Switch(
                "alice".to_string()
            )))
        );
    }

    #[test]
    fn rejects_bare_and_malformed_commands() {
        assert!(
            parse_command("whoami")
                .unwrap_err()
                .contains("start with `/`")
        );
        assert!(parse_command("/whoami").is_err());
        assert!(parse_command("/copy now").is_err());
        assert!(parse_command("/login now").is_err());
        assert!(parse_command("/logout now").is_err());
        assert!(parse_command("/pair https://example.com").is_err());
        assert!(parse_command("/deeplink polkadotapp://pair?handshake=01").is_err());
        assert!(parse_command("/log noisy").is_err());
        assert!(parse_command("/product example.com").is_err());
        assert!(parse_command("/session ../escape").is_err());
        assert!(
            parse_command("/session default")
                .unwrap_err()
                .contains("reserved for bootstrap state")
        );
    }

    #[test]
    fn completion_selection_and_history_have_distinct_arrow_behavior() {
        let mut editor = CommandEditor::default();
        editor.set_text("/");
        let first = editor.completions()[0].value.clone();
        editor.down();
        assert_ne!(editor.completions()[editor.completion_index()].value, first);

        editor.dismiss_completions();
        editor.set_text("/help");
        editor.submit();
        editor.set_text("draft");
        editor.dismiss_completions();
        editor.up();
        assert_eq!(editor.text(), "/help");
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
        let command = completions_for_scope("/script", &[], CommandScope::SigningHost);
        assert_eq!(command.len(), 1);
        assert_eq!(command[0].value, "/script");

        let prefix = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("signing_s");
        let matches = completions_for_scope(
            &format!("/script {}", prefix.display()),
            &[],
            CommandScope::SigningHost,
        );

        assert!(
            matches
                .iter()
                .any(|completion| completion.value.ends_with("signing_shell.rs"))
        );
    }

    #[test]
    fn session_completion_lists_existing_names() {
        let matches = completions_for_scope(
            "/session a",
            &["alice".to_string(), "bob".to_string()],
            CommandScope::SigningHost,
        );

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].value, "/session alice");
    }

    #[test]
    fn pairing_host_completions_only_offer_shared_commands() {
        let matches = completions_for_scope("/", &[], CommandScope::PairingHost);
        let commands = matches
            .into_iter()
            .map(|completion| completion.value)
            .collect::<Vec<_>>();

        assert!(commands.contains(&"/script".to_string()));
        assert!(commands.contains(&"/login".to_string()));
        assert!(commands.contains(&"/logout".to_string()));
        assert!(commands.contains(&"/product".to_string()));
        assert!(commands.contains(&"/copy".to_string()));
        assert!(!commands.iter().any(|command| command.starts_with("/pair")));
        assert!(
            !commands
                .iter()
                .any(|command| command.starts_with("/session"))
        );
    }
}
