//! Terminal ownership, rendering, and host-to-transcript event routing.

use std::collections::VecDeque;
use std::future::Future;
use std::io::{self, IsTerminal, Write};
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures_util::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use tokio::sync::{mpsc, oneshot};
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context as LayerContext, Layer};
use unicode_width::UnicodeWidthStr;

use crate::LogLevel;
use crate::signing_shell::{CommandEditor, parse_approval};

const TRANSCRIPT_LIMIT: usize = 10_000;
const MAX_VISIBLE_COMPLETIONS: usize = 10;

/// Tracing target reserved for SSO summaries that must remain visible at every log level.
pub const SSO_TRANSCRIPT_TARGET: &str = "truapi_server::sso_transcript";

#[derive(Debug, Clone, Copy)]
enum EntryKind {
    Log,
    System,
    Script,
    Command,
    Host,
    User,
    Error,
}

#[derive(Debug)]
struct TranscriptEntry {
    kind: EntryKind,
    text: String,
}

enum UiEvent {
    Entry(TranscriptEntry),
    Approval {
        action: String,
        detail: String,
        response: oneshot::Sender<bool>,
    },
    Connection(String),
    Session {
        name: String,
        available: Vec<String>,
    },
}

static ACTIVE_UI: OnceLock<Mutex<Option<mpsc::UnboundedSender<UiEvent>>>> = OnceLock::new();

fn active_ui() -> &'static Mutex<Option<mpsc::UnboundedSender<UiEvent>>> {
    ACTIVE_UI.get_or_init(|| Mutex::new(None))
}

fn send_to_active(event: UiEvent) -> bool {
    let sender = active_ui().lock().ok().and_then(|active| active.clone());
    sender.is_some_and(|sender| sender.send(event).is_ok())
}

/// Return whether stdin and stdout are both attached to terminals.
pub fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// Write a stable output line, routing it into the active transcript when present.
pub fn output_line(line: impl Into<String>) {
    let line = line.into();
    if !send_to_active(UiEvent::Entry(TranscriptEntry {
        kind: EntryKind::System,
        text: line.clone(),
    })) {
        println!("{line}");
    }
}

/// A tracing writer that redirects complete events into the active transcript.
#[derive(Default)]
pub struct LogWriter {
    buffer: Vec<u8>,
}

impl Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        let text = String::from_utf8_lossy(&self.buffer);
        for line in text.lines().filter(|line| !line.is_empty()) {
            if !send_to_active(UiEvent::Entry(TranscriptEntry {
                kind: EntryKind::Log,
                text: line.to_string(),
            })) {
                let _ = writeln!(io::stderr(), "{line}");
            }
        }
    }
}

/// Unfiltered tracing layer for stable incoming and outgoing SSO summaries.
pub struct SsoTranscriptLayer;

impl<S> Layer<S> for SsoTranscriptLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _context: LayerContext<'_, S>) {
        if event.metadata().target() != SSO_TRANSCRIPT_TARGET {
            return;
        }
        let mut visitor = SsoSummaryVisitor::default();
        event.record(&mut visitor);
        let Some(summary) = visitor.summary else {
            return;
        };
        if !send_to_active(UiEvent::Entry(TranscriptEntry {
            kind: EntryKind::System,
            text: summary.clone(),
        })) {
            let _ = writeln!(io::stderr(), "{summary}");
        }
    }
}

#[derive(Default)]
struct SsoSummaryVisitor {
    summary: Option<String>,
}

impl Visit for SsoSummaryVisitor {
    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "cli_summary" {
            self.summary = Some(value.to_string());
        }
    }
}

/// Cloneable bridge used by the host platform and script runner.
#[derive(Clone)]
pub struct UiHandle {
    sender: mpsc::UnboundedSender<UiEvent>,
}

impl UiHandle {
    /// Add a host lifecycle or result line to the transcript.
    pub fn system(&self, text: impl Into<String>) {
        self.entry(EntryKind::System, text);
    }

    /// Add child-script output to the transcript.
    pub fn script(&self, text: impl Into<String>) {
        self.entry(EntryKind::Script, text);
    }

    /// Update the connection label shown in the transcript panel title.
    pub fn connection(&self, state: impl Into<String>) {
        let _ = self.sender.send(UiEvent::Connection(state.into()));
    }

    /// Update the active session and session-name completions.
    pub fn session(&self, name: impl Into<String>, available: Vec<String>) {
        let _ = self.sender.send(UiEvent::Session {
            name: name.into(),
            available,
        });
    }

    /// Ask the terminal owner for a serialized yes/no decision.
    pub async fn confirm(&self, action: impl Into<String>, detail: impl Into<String>) -> bool {
        let (response, answer) = oneshot::channel();
        if self
            .sender
            .send(UiEvent::Approval {
                action: action.into(),
                detail: detail.into(),
                response,
            })
            .is_err()
        {
            return false;
        }
        answer.await.unwrap_or(false)
    }

    fn entry(&self, kind: EntryKind, text: impl Into<String>) {
        let _ = self.sender.send(UiEvent::Entry(TranscriptEntry {
            kind,
            text: text.into(),
        }));
    }
}

/// Inactive terminal UI whose handle can be installed in the host platform.
pub struct TerminalUi {
    sender: mpsc::UnboundedSender<UiEvent>,
    receiver: mpsc::UnboundedReceiver<UiEvent>,
    app: App,
}

impl TerminalUi {
    /// Create a terminal transcript and its cloneable host bridge.
    pub fn new(
        network: impl Into<String>,
        session: impl Into<String>,
        session_names: Vec<String>,
        log_level: LogLevel,
    ) -> (Self, UiHandle) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let handle = UiHandle {
            sender: sender.clone(),
        };
        (
            Self {
                sender,
                receiver,
                app: App::new(network.into(), session.into(), session_names, log_level),
            },
            handle,
        )
    }

    /// Take exclusive terminal ownership and enter the full-screen UI.
    pub fn enter(self) -> Result<ActiveTerminalUi> {
        let mut active = active_ui()
            .lock()
            .map_err(|_| anyhow::anyhow!("active terminal UI mutex poisoned"))?;
        let terminal = enter_terminal()?;
        *active = Some(self.sender.clone());
        drop(active);
        Ok(ActiveTerminalUi {
            terminal: Some(terminal),
            events: Some(EventStream::new()),
            receiver: self.receiver,
            sender: self.sender,
            app: self.app,
            clipboard: None,
        })
    }
}

type Renderer = Terminal<CrosstermBackend<io::Stdout>>;

/// Result of driving the UI while an operational command runs.
pub enum DriveResult<T> {
    /// The command future completed normally.
    Complete(T),
    /// The user cancelled the command with Ctrl-C.
    Cancelled,
}

/// Full-screen terminal owner used by the signing-host session loop.
pub struct ActiveTerminalUi {
    terminal: Option<Renderer>,
    events: Option<EventStream>,
    receiver: mpsc::UnboundedReceiver<UiEvent>,
    sender: mpsc::UnboundedSender<UiEvent>,
    app: App,
    clipboard: Option<arboard::Clipboard>,
}

impl ActiveTerminalUi {
    /// Return a handle suitable for background host work.
    pub fn handle(&self) -> UiHandle {
        UiHandle {
            sender: self.sender.clone(),
        }
    }

    /// Record a submitted slash command in the transcript.
    pub fn command(&mut self, command: impl Into<String>) {
        self.app.push(EntryKind::Command, command);
    }

    /// Record an immediate system result.
    pub fn system(&mut self, text: impl Into<String>) {
        self.app.push(EntryKind::System, text);
    }

    /// Record an immediate command error.
    pub fn error(&mut self, text: impl Into<String>) {
        self.app.push(EntryKind::Error, text);
    }

    /// Clear the visible transcript.
    pub fn clear(&mut self) {
        self.app.entries.clear();
        self.app.scroll_from_bottom = 0;
    }

    /// Copy the retained transcript to the system clipboard.
    pub fn copy_transcript(&mut self) -> Result<usize> {
        let text = self.app.transcript_text();
        let entries = self.app.entries.len();
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().context("open system clipboard")?);
        }
        self.clipboard
            .as_mut()
            .expect("clipboard was initialized above")
            .set_text(text)
            .context("copy transcript to system clipboard")?;
        Ok(entries)
    }

    /// Update the displayed log level after `/log` succeeds.
    pub fn set_log_level(&mut self, level: LogLevel) {
        self.app.log_level = level;
    }

    /// Yield terminal ownership to an external interactive program.
    pub fn suspend(&mut self) -> Result<()> {
        self.events = None;
        let terminal = self
            .terminal
            .take()
            .context("terminal renderer is unavailable")?;
        if let Err(error) = leave_terminal(terminal) {
            if let Ok(terminal) = enter_terminal() {
                self.terminal = Some(terminal);
                self.events = Some(EventStream::new());
            }
            return Err(error);
        }
        Ok(())
    }

    /// Re-enter the full-screen UI after an external program exits.
    pub fn resume(&mut self) -> Result<()> {
        if self.terminal.is_some() {
            return Ok(());
        }
        self.terminal = Some(enter_terminal()?);
        self.events = Some(EventStream::new());
        Ok(())
    }

    /// Wait for the next submitted command while continuing to render host events.
    pub async fn next_command(&mut self) -> Result<Option<String>> {
        loop {
            self.draw()?;
            tokio::select! {
                event = self.receiver.recv() => {
                    let Some(event) = event else { return Ok(None); };
                    self.app.handle_event(event);
                }
                event = self.events.as_mut().expect("terminal events are active").next() => {
                    let Some(event) = event else { return Ok(None); };
                    let event = event.context("read terminal event")?;
                    if let Some(outcome) = self.app.handle_idle_event(event) {
                        return Ok(outcome);
                    }
                }
            }
        }
    }

    /// Keep rendering and answering approvals while `future` performs one command.
    pub async fn drive<F, T>(
        &mut self,
        label: impl Into<String>,
        future: F,
    ) -> Result<DriveResult<T>>
    where
        F: Future<Output = T>,
    {
        self.app.busy = Some(label.into());
        let mut future = Box::pin(future);
        loop {
            self.draw()?;
            tokio::select! {
                output = &mut future => {
                    self.app.busy = None;
                    return Ok(DriveResult::Complete(output));
                }
                event = self.receiver.recv() => {
                    if let Some(event) = event {
                        self.app.handle_event(event);
                    }
                }
                event = self.events.as_mut().expect("terminal events are active").next() => {
                    let Some(event) = event else {
                        self.app.busy = None;
                        return Ok(DriveResult::Cancelled);
                    };
                    let event = event.context("read terminal event")?;
                    if self.app.handle_busy_event(event) {
                        self.app.busy = None;
                        return Ok(DriveResult::Cancelled);
                    }
                }
            }
        }
    }

    fn draw(&mut self) -> Result<()> {
        let app = &mut self.app;
        self.terminal
            .as_mut()
            .context("terminal renderer is unavailable")?
            .draw(|frame| render(frame, app))
            .context("draw terminal UI")?;
        Ok(())
    }
}

impl Drop for ActiveTerminalUi {
    fn drop(&mut self) {
        if let Ok(mut active) = active_ui().lock() {
            *active = None;
        }
        if let Some(terminal) = self.terminal.take() {
            let _ = leave_terminal(terminal);
        } else {
            let _ = disable_raw_mode();
        }
    }
}

fn enter_terminal() -> Result<Renderer> {
    enable_raw_mode().context("enable terminal raw mode")?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableBracketedPaste, Hide) {
        let _ = disable_raw_mode();
        return Err(error).context("enter alternate terminal screen");
    }
    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, Show, DisableBracketedPaste, LeaveAlternateScreen);
            let _ = disable_raw_mode();
            Err(error).context("initialize terminal renderer")
        }
    }
}

fn leave_terminal(mut terminal: Renderer) -> Result<()> {
    let screen_result = execute!(
        terminal.backend_mut(),
        Show,
        DisableBracketedPaste,
        LeaveAlternateScreen
    )
    .context("leave alternate terminal screen");
    let raw_result = disable_raw_mode().context("disable terminal raw mode");
    screen_result?;
    raw_result
}

struct PendingApproval {
    response: oneshot::Sender<bool>,
    saved_input: String,
}

struct App {
    network: String,
    session: String,
    connection: String,
    log_level: LogLevel,
    entries: VecDeque<TranscriptEntry>,
    editor: CommandEditor,
    pending_approval: Option<PendingApproval>,
    busy: Option<String>,
    scroll_from_bottom: usize,
    transcript_height: usize,
}

impl App {
    fn new(
        network: String,
        session: String,
        session_names: Vec<String>,
        log_level: LogLevel,
    ) -> Self {
        let mut editor = CommandEditor::default();
        editor.set_session_names(session_names);
        Self {
            network,
            session,
            connection: "disconnected".to_string(),
            log_level,
            entries: VecDeque::new(),
            editor,
            pending_approval: None,
            busy: None,
            scroll_from_bottom: 0,
            transcript_height: 1,
        }
    }

    fn push(&mut self, kind: EntryKind, text: impl Into<String>) {
        if self.entries.len() == TRANSCRIPT_LIMIT {
            self.entries.pop_front();
        }
        self.entries.push_back(TranscriptEntry {
            kind,
            text: text.into(),
        });
    }

    fn transcript_text(&self) -> String {
        self.entries
            .iter()
            .map(|entry| format!("{}{}", entry_plain_label(entry.kind), entry.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn handle_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Entry(entry) => self.push(entry.kind, entry.text),
            UiEvent::Connection(state) => self.connection = state,
            UiEvent::Session { name, available } => {
                self.session = name;
                self.editor.set_session_names(available);
            }
            UiEvent::Approval {
                action,
                detail,
                response,
            } => {
                if self.pending_approval.is_some() {
                    let _ = response.send(false);
                    self.push(EntryKind::Error, "rejected overlapping approval prompt");
                    return;
                }
                let saved_input = self.editor.text();
                self.editor.clear();
                self.push(
                    EntryKind::Host,
                    format!("Approval required\n{action}\n{detail}\nAnswer yes or no."),
                );
                self.pending_approval = Some(PendingApproval {
                    response,
                    saved_input,
                });
            }
        }
    }

    fn handle_idle_event(&mut self, event: Event) -> Option<Option<String>> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if self.pending_approval.is_some() {
                    self.handle_approval_key(key);
                    return None;
                }
                self.handle_common_key(key, false)
            }
            Event::Paste(text) => {
                self.insert_paste(&text);
                None
            }
            _ => None,
        }
    }

    fn handle_busy_event(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if self.pending_approval.is_some() {
                    self.handle_approval_key(key);
                    return false;
                }
                self.handle_common_key(key, true)
                    .is_some_and(|value| value.is_none())
            }
            Event::Paste(text) => {
                self.insert_paste(&text);
                false
            }
            _ => false,
        }
    }

    fn insert_paste(&mut self, text: &str) {
        for character in text.chars().filter(|character| !character.is_control()) {
            self.editor.insert(character);
        }
    }

    fn handle_common_key(&mut self, key: KeyEvent, busy: bool) -> Option<Option<String>> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match (control, key.code) {
            (true, KeyCode::Char('u')) => self.scroll_up(),
            (true, KeyCode::Char('d')) => self.scroll_down(),
            (true, KeyCode::Char('c')) => {
                if !self.editor.text().is_empty() {
                    self.editor.clear();
                } else {
                    return Some(None);
                }
            }
            (false, KeyCode::Char(character)) => self.editor.insert(character),
            (false, KeyCode::Backspace) => self.editor.backspace(),
            (false, KeyCode::Delete) => self.editor.delete(),
            (false, KeyCode::Left) => self.editor.left(),
            (false, KeyCode::Right) => self.editor.right(),
            (false, KeyCode::Home) => self.editor.home(),
            (false, KeyCode::End) => {
                self.editor.end();
                self.scroll_from_bottom = 0;
            }
            (false, KeyCode::Up) => self.editor.up(),
            (false, KeyCode::Down) => self.editor.down(),
            (false, KeyCode::Esc) => self.editor.dismiss_completions(),
            (false, KeyCode::Tab) => {
                self.editor.accept_completion();
            }
            (false, KeyCode::Enter) if busy => {
                if !self.editor.text().trim().is_empty() {
                    self.push(EntryKind::Error, "a command is already running");
                }
            }
            (false, KeyCode::Enter) => {
                let text = self.editor.text();
                let completions = self.editor.completions();
                if let Some(completion) = completions.get(self.editor.completion_index())
                    && text != completion.value
                {
                    self.editor.accept_completion();
                    return None;
                }
                let command = self.editor.submit();
                if !command.trim().is_empty() {
                    return Some(Some(command));
                }
            }
            _ => {}
        }
        None
    }

    fn handle_approval_key(&mut self, key: KeyEvent) {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match (control, key.code) {
            (true, KeyCode::Char('u')) => self.scroll_up(),
            (true, KeyCode::Char('d')) => self.scroll_down(),
            (false, KeyCode::Esc) => self.answer_approval(false),
            (false, KeyCode::Enter) => {
                let answer = self.editor.text();
                match parse_approval(&answer) {
                    Some(answer) => self.answer_approval(answer),
                    None => {
                        self.editor.clear();
                        self.push(EntryKind::Error, "answer yes or no");
                    }
                }
            }
            (true, KeyCode::Char('c')) => self.editor.clear(),
            (false, KeyCode::Char(character)) => self.editor.insert(character),
            (false, KeyCode::Backspace) => self.editor.backspace(),
            (false, KeyCode::Delete) => self.editor.delete(),
            (false, KeyCode::Left) => self.editor.left(),
            (false, KeyCode::Right) => self.editor.right(),
            (false, KeyCode::Home) => self.editor.home(),
            (false, KeyCode::End) => self.editor.end(),
            _ => {}
        }
    }

    fn answer_approval(&mut self, approved: bool) {
        let Some(pending) = self.pending_approval.take() else {
            return;
        };
        self.editor.clear();
        self.push(EntryKind::User, if approved { "yes" } else { "no" });
        self.push(
            EntryKind::Host,
            if approved { "Approved" } else { "Rejected" },
        );
        self.editor.set_text(pending.saved_input);
        let _ = pending.response.send(approved);
    }

    fn scroll_up(&mut self) {
        self.scroll_from_bottom = self
            .scroll_from_bottom
            .saturating_add((self.transcript_height / 2).max(1));
    }

    fn scroll_down(&mut self) {
        self.scroll_from_bottom = self
            .scroll_from_bottom
            .saturating_sub((self.transcript_height / 2).max(1));
    }
}

fn render(frame: &mut ratatui::Frame<'_>, app: &mut App) {
    let completions = if app.pending_approval.is_some() {
        Vec::new()
    } else {
        app.editor.completions()
    };
    let completion_range = completion_window(completions.len(), app.editor.completion_index());
    let completion_height = if completions.is_empty() {
        0
    } else {
        (completion_range.len() + 2) as u16
    };
    let areas = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(completion_height),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(frame.area());
    app.transcript_height = areas[0].height.saturating_sub(2) as usize;

    let transcript_lines = app.entries.iter().flat_map(entry_lines).collect::<Vec<_>>();
    let content_height = transcript_lines.len();
    let top = content_height
        .saturating_sub(app.transcript_height)
        .saturating_sub(app.scroll_from_bottom)
        .min(u16::MAX as usize) as u16;
    let title = format!(
        " TrUAPI signing host · {} · session {} · {} · {} ",
        app.network, app.session, app.connection, app.log_level
    );
    frame.render_widget(
        Paragraph::new(transcript_lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((top, 0)),
        areas[0],
    );

    if !completions.is_empty() {
        let selected = app.editor.completion_index();
        let items = completions
            .iter()
            .skip(completion_range.start)
            .take(completion_range.len())
            .map(|completion| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        completion.value.clone(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(completion.description, Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default()
            .with_selected(Some(selected.saturating_sub(completion_range.start)));
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::default()
                        .title(" autocomplete ")
                        .borders(Borders::ALL),
                )
                .highlight_symbol("› ")
                .highlight_style(Style::default().bg(Color::DarkGray)),
            areas[1],
            &mut state,
        );
    }

    let approval = app.pending_approval.is_some();
    let input_title = if approval {
        " answer · yes/no "
    } else {
        " command "
    };
    let input = app.editor.text();
    frame.render_widget(
        Paragraph::new(format!("› {input}"))
            .style(if approval {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            })
            .block(Block::default().title(input_title).borders(Borders::ALL)),
        areas[2],
    );
    let cursor_prefix = input.chars().take(app.editor.cursor()).collect::<String>();
    let cursor_x = areas[2]
        .x
        .saturating_add(2)
        .saturating_add(UnicodeWidthStr::width(cursor_prefix.as_str()) as u16)
        .min(areas[2].right().saturating_sub(2));
    frame.set_cursor_position((cursor_x, areas[2].y + 1));

    let busy = app
        .busy
        .as_deref()
        .map(|command| format!("running {command} · "))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(format!(
            "{busy}↑↓ autocomplete/history · Tab complete · Ctrl-U/D scroll · Enter run"
        ))
        .style(Style::default().fg(Color::DarkGray)),
        areas[3],
    );
}

fn completion_window(total: usize, selected: usize) -> Range<usize> {
    if total <= MAX_VISIBLE_COMPLETIONS {
        return 0..total;
    }
    let start = selected
        .saturating_add(1)
        .saturating_sub(MAX_VISIBLE_COMPLETIONS)
        .min(total - MAX_VISIBLE_COMPLETIONS);
    start..start + MAX_VISIBLE_COMPLETIONS
}

fn entry_lines(entry: &TranscriptEntry) -> Vec<Line<'static>> {
    let (label, style) = match entry.kind {
        EntryKind::Log => ("", Style::default().fg(Color::Gray)),
        EntryKind::System => ("HOST · ", Style::default().fg(Color::Green)),
        EntryKind::Script => ("SCRIPT · ", Style::default().fg(Color::Magenta)),
        EntryKind::Command => (
            "YOU · ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        EntryKind::Host => (
            "HOST · ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        EntryKind::User => (
            "YOU · ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        EntryKind::Error => (
            "ERROR · ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    };
    entry
        .text
        .lines()
        .enumerate()
        .map(|(index, text)| {
            let prefix = if index == 0 { label } else { "       " };
            Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(text.to_string(), style),
            ])
        })
        .collect()
}

fn entry_plain_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Log => "",
        EntryKind::System | EntryKind::Host => "HOST · ",
        EntryKind::Script => "SCRIPT · ",
        EntryKind::Command | EntryKind::User => "YOU · ",
        EntryKind::Error => "ERROR · ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    fn test_app() -> App {
        App::new(
            "testnet".to_string(),
            "default".to_string(),
            vec!["default".to_string()],
            LogLevel::Info,
        )
    }

    #[test]
    fn approval_temporarily_replaces_and_then_restores_command_draft() {
        let mut app = test_app();
        app.editor.set_text("/script draft.ts");
        let (response, answer) = oneshot::channel();
        app.handle_event(UiEvent::Approval {
            action: "sign request".to_string(),
            detail: "payload".to_string(),
            response,
        });
        app.editor.set_text("YES");
        app.handle_approval_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(answer.blocking_recv(), Ok(true));
        assert_eq!(app.editor.text(), "/script draft.ts");
        assert!(app.pending_approval.is_none());
    }

    #[test]
    fn ctrl_u_and_ctrl_d_scroll_half_a_viewport() {
        let mut app = test_app();
        app.transcript_height = 20;
        app.handle_common_key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            false,
        );
        assert_eq!(app.scroll_from_bottom, 10);
        app.handle_common_key(
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            false,
        );
        assert_eq!(app.scroll_from_bottom, 0);
    }

    #[test]
    fn transcript_is_bounded() {
        let mut app = test_app();
        for index in 0..=TRANSCRIPT_LIMIT {
            app.push(EntryKind::Log, index.to_string());
        }
        assert_eq!(app.entries.len(), TRANSCRIPT_LIMIT);
        assert_eq!(
            app.entries.front().map(|entry| entry.text.as_str()),
            Some("1")
        );
    }

    #[test]
    fn transcript_text_includes_visible_speaker_labels() {
        let mut app = test_app();
        app.push(EntryKind::System, "ready");
        app.push(EntryKind::Command, "/script demo.ts");
        app.push(EntryKind::Script, "user id: alice.dot");

        assert_eq!(
            app.transcript_text(),
            "HOST · ready\nYOU · /script demo.ts\nSCRIPT · user id: alice.dot"
        );
    }

    #[test]
    fn autocomplete_shows_all_current_commands_and_scrolls_larger_menus() {
        let mut app = test_app();
        app.editor.set_text("/");
        let completions = app.editor.completions();
        let visible = completion_window(completions.len(), app.editor.completion_index());
        assert!(
            completions[visible]
                .iter()
                .any(|completion| completion.value == "/copy")
        );

        assert_eq!(completion_window(12, 0), 0..10);
        assert_eq!(completion_window(12, 11), 2..12);
    }

    #[test]
    fn session_event_updates_header_state_and_completions() {
        let mut app = test_app();
        app.handle_event(UiEvent::Session {
            name: "alice".to_string(),
            available: vec!["alice".to_string(), "bob".to_string()],
        });
        app.editor.set_text("/session b");

        assert_eq!(app.session, "alice");
        assert_eq!(app.editor.completions()[0].value, "/session bob");
    }

    #[test]
    fn sso_summary_bypasses_the_adjustable_log_filter() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        *active_ui().lock().expect("lock active test UI") = Some(sender);
        let filtered_logs = tracing_subscriber::fmt::layer()
            .with_writer(io::sink)
            .with_filter(tracing_subscriber::EnvFilter::new("error"));
        let subscriber = tracing_subscriber::registry()
            .with(SsoTranscriptLayer)
            .with(filtered_logs);

        tracing::subscriber::with_default(subscriber, || {
            tracing::event!(
                target: SSO_TRANSCRIPT_TARGET,
                tracing::Level::INFO,
                cli_summary = "SSO response sent · get_account_alias · ok"
            );
        });
        *active_ui().lock().expect("unlock active test UI") = None;

        let UiEvent::Entry(entry) = receiver.try_recv().expect("summary transcript event") else {
            panic!("expected transcript entry");
        };
        assert_eq!(entry.text, "SSO response sent · get_account_alias · ok");
    }
}
