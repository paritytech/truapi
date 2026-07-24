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
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap};
use tokio::sync::{mpsc, oneshot};
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context as LayerContext, Layer};
use unicode_width::UnicodeWidthChar;

use crate::LogLevel;
use crate::signing_shell::{CommandEditor, parse_approval};

const TRANSCRIPT_LIMIT: usize = 10_000;
const TRANSCRIPT_LINE_LIMIT: usize = 10_000;
const TRANSCRIPT_BYTE_LIMIT: usize = 1024 * 1024;
const STREAM_CHUNK_LINE_LIMIT: usize = 256;
const STREAM_CHUNK_BYTE_LIMIT: usize = 64 * 1024;
const STREAM_LINE_BYTE_LIMIT: usize = 16 * 1024;
const EVENT_BATCH_LIMIT: usize = 256;
const MAX_VISIBLE_COMPLETIONS: usize = 8;
const COMPOSER_HORIZONTAL_PADDING: u16 = 1;

/// Tracing target reserved for SSO summaries that must remain visible at every log level.
pub const SSO_TRANSCRIPT_TARGET: &str = "truapi_server::sso_transcript";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoticeTone {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    Running,
    Succeeded,
    Warning,
    Failed,
    Cancelled,
}

#[derive(Debug)]
enum FeedItem {
    Log(String),
    Notice {
        tone: NoticeTone,
        title: String,
        detail: Option<String>,
    },
    Command(String),
    Stream {
        kind: StreamKind,
        lines: Vec<String>,
    },
    Activity {
        id: u64,
        key: String,
        label: String,
        detail: Option<String>,
        state: ActivityState,
    },
    Approval {
        id: u64,
        action: String,
        detail: String,
        outcome: Option<bool>,
    },
    Request {
        key: String,
        name: String,
        state: ActivityState,
        metadata: Option<String>,
        elapsed_ms: Option<u64>,
        reason: Option<String>,
    },
}

/// Stable lifecycle events with separate machine and interactive presentations.
#[derive(Debug, Clone)]
pub enum SystemEvent {
    FramesListening {
        url: String,
    },
    SigningHostReady,
    SigningHostNeedsSession,
    SigningHostAccountExhausted {
        name: String,
        period: u32,
    },
    SigningHostExit {
        outcome: String,
    },
    SigningHostError {
        reason: String,
    },
    SigningHostResponderStarted,
    RingInfo {
        ring_index: u32,
        members: usize,
    },
    AllowanceChecking {
        target: String,
    },
    AllowanceReady {
        target: String,
        sequence: u32,
        block_hash: Option<String>,
        already_allocated: bool,
    },
    NotificationDelivered {
        id: u32,
        text: String,
        deeplink: Option<String>,
    },
    NotificationScheduled {
        id: u32,
        text: String,
        scheduled_at: u64,
    },
    NotificationCancelled {
        id: u32,
    },
    PairingDeeplink {
        url: String,
    },
    PairingAuthenticating,
    PairingConnected {
        user_id: Option<String>,
    },
    PairingDisconnected,
    PairingFailed {
        reason: String,
    },
    ScriptStarted,
    ScriptExit {
        code: i32,
    },
    SessionStatus {
        name: String,
        path: String,
        user_id: String,
    },
    SessionSwitching {
        from: String,
        to: String,
    },
    SessionCreating {
        name: String,
    },
    LogLevelChanged {
        level: LogLevel,
    },
    CopiedTranscript {
        entries: usize,
    },
}

impl SystemEvent {
    /// Render the same sentence-case copy used by the interactive transcript.
    pub fn human(&self) -> String {
        let mut app = App::new_pairing(String::new(), String::new(), LogLevel::Info);
        app.connection = "connected".to_string();
        app.handle_system_event(self.clone());
        let text = app.transcript_text();
        match self {
            Self::PairingDeeplink { url } => text.replace("<pairing link>", url),
            _ => text,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SsoEvent {
    kind: Option<String>,
    request: Option<String>,
    statement_request_id: Option<String>,
    remote_message_id: Option<String>,
    response_message_id: Option<String>,
    outcome: Option<String>,
    elapsed_ms: Option<u64>,
    reason: Option<String>,
    fallback_summary: Option<String>,
}

enum UiEvent {
    Log(String),
    Notice {
        tone: NoticeTone,
        title: String,
        detail: Option<String>,
    },
    Stream {
        kind: StreamKind,
        text: String,
    },
    System(SystemEvent),
    Activity {
        key: String,
        label: String,
        detail: Option<String>,
        state: ActivityState,
    },
    Sso(SsoEvent),
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

/// Emit a lifecycle event using the same human copy in every presentation.
pub fn output_event(event: SystemEvent) {
    if !send_to_active(UiEvent::System(event.clone())) {
        let text = event.human();
        if !text.is_empty() {
            write_human_stdout(&text);
        }
    }
}

/// Emit a success notice through the active transcript or streaming renderer.
pub fn output_success(title: impl Into<String>, detail: Option<String>) {
    let title = title.into();
    if !send_to_active(UiEvent::Notice {
        tone: NoticeTone::Success,
        title: title.clone(),
        detail: detail.clone(),
    }) {
        let mut app = App::new_pairing(String::new(), String::new(), LogLevel::Info);
        app.notice(NoticeTone::Success, title, detail);
        write_human_stdout(&app.transcript_text());
    }
}

fn write_human_stdout(text: &str) {
    let styled = io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let mut stdout = io::stdout().lock();
    for line in text.lines() {
        if !styled {
            let _ = writeln!(stdout, "{line}");
            continue;
        }
        let color = match line.chars().next() {
            Some('✓') => "\x1b[32m",
            Some('×') => "\x1b[31m",
            Some('!') => "\x1b[33m",
            Some('•' | '◌') => "\x1b[36m",
            _ if line.starts_with("  ") => "\x1b[2m",
            _ => "",
        };
        if color.is_empty() {
            let _ = writeln!(stdout, "{line}");
        } else {
            let _ = writeln!(stdout, "{color}{line}\x1b[0m");
        }
    }
}

/// Update a keyed activity in the active TUI without affecting plain output.
pub fn update_activity(
    key: impl Into<String>,
    label: impl Into<String>,
    detail: Option<String>,
    state: ActivityState,
) {
    let _ = send_to_active(UiEvent::Activity {
        key: key.into(),
        label: label.into(),
        detail,
        state,
    });
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
            if !send_to_active(UiEvent::Log(sanitize_terminal_text(line))) {
                let mut stderr = io::stderr();
                if stderr.is_terminal() && std::env::var_os("NO_COLOR").is_none() {
                    let _ = writeln!(stderr, "\x1b[2m{line}\x1b[0m");
                } else {
                    let _ = writeln!(stderr, "{line}");
                }
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
        if visitor.event.fallback_summary.is_none() {
            return;
        }
        if !send_to_active(UiEvent::Sso(visitor.event.clone())) {
            write_human_stderr(&sso_event_text(visitor.event));
        }
    }
}

fn sso_event_text(event: SsoEvent) -> String {
    let mut app = App::new_pairing(String::new(), String::new(), LogLevel::Info);
    app.handle_sso_event(event);
    app.transcript_text()
}

fn write_human_stderr(text: &str) {
    let styled = io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let mut stderr = io::stderr().lock();
    for line in text.lines() {
        if !styled {
            let _ = writeln!(stderr, "{line}");
            continue;
        }
        let color = match line.chars().next() {
            Some('✓') => "\x1b[32m",
            Some('×') => "\x1b[31m",
            Some('!') => "\x1b[33m",
            Some('•' | '◌') => "\x1b[36m",
            _ if line.starts_with("  ") => "\x1b[2m",
            _ => "",
        };
        if color.is_empty() {
            let _ = writeln!(stderr, "{line}");
        } else {
            let _ = writeln!(stderr, "{color}{line}\x1b[0m");
        }
    }
}

#[derive(Default)]
struct SsoSummaryVisitor {
    event: SsoEvent,
}

impl Visit for SsoSummaryVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value = format!("{value:?}").trim_matches('"').to_string();
        self.record_value(field.name(), value);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field.name(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "elapsed_ms" {
            self.event.elapsed_ms = Some(value);
        }
    }
}

impl SsoSummaryVisitor {
    fn record_value(&mut self, name: &str, value: String) {
        match name {
            "cli_summary" => self.event.fallback_summary = Some(value),
            "cli_event" => self.event.kind = Some(value),
            "request" | "remote_message" => self.event.request = Some(value),
            "statement_request_id" => self.event.statement_request_id = Some(value),
            "remote_message_id" | "responding_to" => {
                self.event.remote_message_id = Some(value);
            }
            "response_message_id" => self.event.response_message_id = Some(value),
            "outcome" => self.event.outcome = Some(value),
            "reason" if !value.is_empty() => self.event.reason = Some(value),
            _ => {}
        }
    }
}

/// Cloneable bridge used by the host platform and script runner.
#[derive(Clone)]
pub struct UiHandle {
    sender: mpsc::UnboundedSender<UiEvent>,
}

impl UiHandle {
    /// Add a successful human-facing outcome to the transcript.
    pub fn success(&self, title: impl Into<String>, detail: Option<String>) {
        let _ = self.sender.send(UiEvent::Notice {
            tone: NoticeTone::Success,
            title: title.into(),
            detail,
        });
    }

    /// Add a typed lifecycle event to the transcript.
    pub fn event(&self, event: SystemEvent) {
        let _ = self.sender.send(UiEvent::System(event));
    }

    /// Add child-script stdout to the active command block.
    pub fn script_stdout(&self, text: impl Into<String>) {
        let _ = self.sender.send(UiEvent::Stream {
            kind: StreamKind::Stdout,
            text: sanitize_terminal_text(&text.into()),
        });
    }

    /// Add child-script stderr to the active command block.
    pub fn script_stderr(&self, text: impl Into<String>) {
        let _ = self.sender.send(UiEvent::Stream {
            kind: StreamKind::Stderr,
            text: sanitize_terminal_text(&text.into()),
        });
    }

    /// Update the user or transitional auth label shown in the status bar.
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
        product_id: impl Into<String>,
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
                app: App::new(
                    network.into(),
                    product_id.into(),
                    session.into(),
                    session_names,
                    log_level,
                ),
            },
            handle,
        )
    }

    /// Create the same terminal surface for a product-side pairing host.
    pub fn new_pairing(
        network: impl Into<String>,
        product_id: impl Into<String>,
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
                app: App::new_pairing(network.into(), product_id.into(), log_level),
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
            copy_next_pairing_deeplink: false,
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
    copy_next_pairing_deeplink: bool,
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
        self.app.push_command(command.into());
    }

    /// Record an immediate system result.
    pub fn system(&mut self, text: impl Into<String>) {
        self.app.notice(NoticeTone::Info, text.into(), None);
    }

    /// Record an immediate successful result.
    pub fn success(&mut self, text: impl Into<String>, detail: Option<String>) {
        self.app.notice(NoticeTone::Success, text.into(), detail);
    }

    /// Record a typed lifecycle event.
    pub fn event(&mut self, event: SystemEvent) {
        self.app.handle_system_event(event);
    }

    /// Record an immediate command error.
    pub fn error(&mut self, text: impl Into<String>) {
        self.app.notice(NoticeTone::Error, text.into(), None);
    }

    /// Clear the visible transcript.
    pub fn clear(&mut self) {
        self.app.entries.clear();
        self.app.retained_lines = 0;
        self.app.retained_bytes = 0;
        self.app.scroll_from_bottom = 0;
    }

    /// Copy the retained transcript to the system clipboard.
    pub fn copy_transcript(&mut self) -> Result<usize> {
        let text = self.app.transcript_text();
        let entries = self.app.entries.len();
        self.copy_text(text, "copy transcript to system clipboard")?;
        Ok(entries)
    }

    fn copy_text(&mut self, text: String, context: &'static str) -> Result<()> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().context("open system clipboard")?);
        }
        self.clipboard
            .as_mut()
            .expect("clipboard was initialized above")
            .set_text(text)
            .context(context)
    }

    /// Update the displayed log level after `/log` succeeds.
    pub fn set_log_level(&mut self, level: LogLevel) {
        self.app.log_level = level;
    }

    /// Update the product shown in the host status bar.
    pub fn set_product(&mut self, product_id: impl Into<String>) {
        self.app.product = product_id.into();
    }

    /// Return an activity id boundary for one operational command.
    pub fn activity_checkpoint(&self) -> u64 {
        self.app.next_item_id
    }

    /// Finalize activities started by the current command after an error or cancellation.
    pub fn finish_activities_since(&mut self, checkpoint: u64, state: ActivityState, detail: &str) {
        self.app
            .finish_running_activities_since(checkpoint, state, detail);
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
                    self.handle_ui_event(event);
                    self.drain_pending_events();
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
        self.app.busy = Some(redact_command(&label.into()));
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
                        self.handle_ui_event(event);
                        self.drain_pending_events();
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

    /// Drive `/login` while copying the first typed pairing deeplink event.
    pub async fn drive_pairing_login<F, T>(
        &mut self,
        label: impl Into<String>,
        future: F,
    ) -> Result<DriveResult<T>>
    where
        F: Future<Output = T>,
    {
        self.copy_next_pairing_deeplink = true;
        let result = self.drive(label, future).await;
        self.copy_next_pairing_deeplink = false;
        result
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

    fn drain_pending_events(&mut self) {
        for _ in 1..EVENT_BATCH_LIMIT {
            let Ok(event) = self.receiver.try_recv() else {
                break;
            };
            self.handle_ui_event(event);
        }
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        let deeplink = pairing_deeplink_to_copy(self.copy_next_pairing_deeplink, &event)
            .map(ToOwned::to_owned);
        self.app.handle_event(event);
        let Some(deeplink) = deeplink else {
            return;
        };
        self.copy_next_pairing_deeplink = false;
        match self.copy_text(deeplink, "copy pairing link to system clipboard") {
            Ok(()) => self.app.notice(
                NoticeTone::Success,
                "Pairing link copied".to_string(),
                Some("Clipboard updated".to_string()),
            ),
            Err(error) => self.app.notice(
                NoticeTone::Warning,
                "Could not copy pairing link".to_string(),
                Some(error.to_string()),
            ),
        }
    }
}

fn pairing_deeplink_to_copy(enabled: bool, event: &UiEvent) -> Option<&str> {
    if !enabled {
        return None;
    }
    match event {
        UiEvent::System(SystemEvent::PairingDeeplink { url }) => Some(url),
        _ => None,
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
    id: u64,
    response: oneshot::Sender<bool>,
    saved_input: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostRole {
    Pairing,
    Signing,
}

struct App {
    role: HostRole,
    network: String,
    product: String,
    session: String,
    connection: String,
    log_level: LogLevel,
    entries: VecDeque<FeedItem>,
    editor: CommandEditor,
    pending_approval: Option<PendingApproval>,
    busy: Option<String>,
    scroll_from_bottom: usize,
    transcript_height: usize,
    next_item_id: u64,
    retained_lines: usize,
    retained_bytes: usize,
}

impl App {
    fn new(
        network: String,
        product_id: String,
        session: String,
        session_names: Vec<String>,
        log_level: LogLevel,
    ) -> Self {
        Self::with_role(
            HostRole::Signing,
            network,
            product_id,
            session,
            session_names,
            log_level,
        )
    }

    fn new_pairing(network: String, product_id: String, log_level: LogLevel) -> Self {
        Self::with_role(
            HostRole::Pairing,
            network,
            product_id,
            String::new(),
            Vec::new(),
            log_level,
        )
    }

    fn with_role(
        role: HostRole,
        network: String,
        product: String,
        session: String,
        session_names: Vec<String>,
        log_level: LogLevel,
    ) -> Self {
        let mut editor = match role {
            HostRole::Pairing => CommandEditor::pairing_host(),
            HostRole::Signing => CommandEditor::default(),
        };
        editor.set_session_names(session_names);
        Self {
            role,
            network,
            product,
            session,
            connection: "disconnected".to_string(),
            log_level,
            entries: VecDeque::new(),
            editor,
            pending_approval: None,
            busy: None,
            scroll_from_bottom: 0,
            transcript_height: 1,
            next_item_id: 1,
            retained_lines: 0,
            retained_bytes: 0,
        }
    }

    fn push(&mut self, item: FeedItem) {
        let (lines, bytes) = feed_item_cost(&item);
        self.retained_lines = self.retained_lines.saturating_add(lines);
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        self.entries.push_back(item);
        self.prune_transcript();
    }

    fn prune_transcript(&mut self) {
        while self.entries.len() > TRANSCRIPT_LIMIT
            || self.retained_lines > TRANSCRIPT_LINE_LIMIT
            || self.retained_bytes > TRANSCRIPT_BYTE_LIMIT
        {
            let Some(item) = self.entries.pop_front() else {
                break;
            };
            let (lines, bytes) = feed_item_cost(&item);
            self.retained_lines = self.retained_lines.saturating_sub(lines);
            self.retained_bytes = self.retained_bytes.saturating_sub(bytes);
        }
    }

    fn recalculate_retained(&mut self) {
        let (lines, bytes) = self.entries.iter().map(feed_item_cost).fold(
            (0_usize, 0_usize),
            |(lines, bytes), (item_lines, item_bytes)| {
                (
                    lines.saturating_add(item_lines),
                    bytes.saturating_add(item_bytes),
                )
            },
        );
        self.retained_lines = lines;
        self.retained_bytes = bytes;
        self.prune_transcript();
    }

    fn push_command(&mut self, command: String) {
        self.push(FeedItem::Command(redact_command(&command)));
    }

    fn notice(&mut self, tone: NoticeTone, title: String, detail: Option<String>) {
        self.push(FeedItem::Notice {
            tone,
            title: sanitize_terminal_text(&title),
            detail: detail.map(|value| sanitize_terminal_text(&value)),
        });
    }

    fn stream(&mut self, kind: StreamKind, text: String) {
        let text = truncate_utf8(&text, STREAM_LINE_BYTE_LIMIT);
        let text_bytes = text.len();
        let can_append = self.entries.back().is_some_and(|item| {
            matches!(
                item,
                FeedItem::Stream {
                    kind: previous_kind,
                    lines,
                } if *previous_kind == kind
                    && lines.len() < STREAM_CHUNK_LINE_LIMIT
                    && lines.iter().map(String::len).sum::<usize>() + text_bytes
                        <= STREAM_CHUNK_BYTE_LIMIT
            )
        });
        if can_append {
            let Some(FeedItem::Stream { lines, .. }) = self.entries.back_mut() else {
                unreachable!("stream append was checked above");
            };
            lines.push(text);
            self.retained_lines = self.retained_lines.saturating_add(1);
            self.retained_bytes = self.retained_bytes.saturating_add(text_bytes);
            self.prune_transcript();
            return;
        }
        self.push(FeedItem::Stream {
            kind,
            lines: vec![text],
        });
    }

    fn activity(
        &mut self,
        key: String,
        label: String,
        detail: Option<String>,
        state: ActivityState,
    ) {
        let label = sanitize_terminal_text(&label);
        let detail = detail.map(|value| sanitize_terminal_text(&value));
        let updated_cost = self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| {
                matches!(
                    entry,
                    FeedItem::Activity {
                        key: current,
                        state: ActivityState::Running,
                        ..
                    } if current == &key
                )
            })
            .map(|entry| {
                let before = feed_item_cost(entry);
                let FeedItem::Activity {
                    label: current_label,
                    detail: current_detail,
                    state: current_state,
                    ..
                } = entry
                else {
                    unreachable!("activity lookup matched a different item");
                };
                *current_label = label.clone();
                *current_detail = detail.clone();
                *current_state = state;
                (before, feed_item_cost(entry))
            });
        if let Some(((before_lines, before_bytes), (after_lines, after_bytes))) = updated_cost {
            self.retained_lines = self
                .retained_lines
                .saturating_sub(before_lines)
                .saturating_add(after_lines);
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(before_bytes)
                .saturating_add(after_bytes);
            self.prune_transcript();
            return;
        }
        let id = self.allocate_item_id();
        self.push(FeedItem::Activity {
            id,
            key,
            label,
            detail,
            state,
        });
    }

    fn start_activity(&mut self, key: String, label: String, detail: Option<String>) {
        self.finish_activity(
            &key,
            ActivityState::Cancelled,
            Some("Superseded".to_string()),
        );
        let id = self.allocate_item_id();
        self.push(FeedItem::Activity {
            id,
            key,
            label: sanitize_terminal_text(&label),
            detail: detail.map(|value| sanitize_terminal_text(&value)),
            state: ActivityState::Running,
        });
    }

    fn finish_activity(&mut self, key: &str, state: ActivityState, detail: Option<String>) {
        let Some(FeedItem::Activity {
            label,
            detail: current_detail,
            ..
        }) = self.entries.iter().rev().find(
            |entry| matches!(entry, FeedItem::Activity { key: current, state: ActivityState::Running, .. } if current == key),
        )
        else {
            return;
        };
        self.activity(
            key.to_string(),
            label.clone(),
            detail.or_else(|| current_detail.clone()),
            state,
        );
    }

    fn finish_running_activities_since(
        &mut self,
        first_id: u64,
        state: ActivityState,
        detail: &str,
    ) {
        let keys = self
            .entries
            .iter()
            .filter_map(|entry| match entry {
                FeedItem::Activity {
                    id,
                    key,
                    state: ActivityState::Running,
                    ..
                } if *id >= first_id => Some(key.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for key in keys {
            self.finish_activity(&key, state, Some(detail.to_string()));
        }
    }

    fn allocate_item_id(&mut self) -> u64 {
        let id = self.next_item_id;
        self.next_item_id += 1;
        id
    }

    fn transcript_text(&self) -> String {
        self.entries
            .iter()
            .flat_map(feed_item_plain_lines)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn handle_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Log(text) => self.push(FeedItem::Log(text)),
            UiEvent::Notice {
                tone,
                title,
                detail,
            } => self.notice(tone, title, detail),
            UiEvent::Stream { kind, text } => self.stream(kind, text),
            UiEvent::System(event) => self.handle_system_event(event),
            UiEvent::Activity {
                key,
                label,
                detail,
                state,
            } => self.activity(key, label, detail, state),
            UiEvent::Sso(event) => self.handle_sso_event(event),
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
                    self.notice(
                        NoticeTone::Error,
                        "Rejected an overlapping approval request".to_string(),
                        None,
                    );
                    return;
                }
                let saved_input = self.editor.text();
                self.editor.clear();
                let id = self.allocate_item_id();
                self.push(FeedItem::Approval {
                    id,
                    action: sanitize_terminal_text(&action),
                    detail: sanitize_terminal_text(&detail),
                    outcome: None,
                });
                self.pending_approval = Some(PendingApproval {
                    id,
                    response,
                    saved_input,
                });
            }
        }
    }

    fn handle_system_event(&mut self, event: SystemEvent) {
        match event {
            SystemEvent::FramesListening { url } => self.notice(
                NoticeTone::Info,
                "Listening for product frames".to_string(),
                Some(url),
            ),
            SystemEvent::SigningHostReady => self.activity(
                "signer".to_string(),
                "Signing host ready".to_string(),
                None,
                ActivityState::Succeeded,
            ),
            SystemEvent::SigningHostNeedsSession => self.notice(
                NoticeTone::Warning,
                "No connected user".to_string(),
                Some("Use /session <name> to start a session.".to_string()),
            ),
            SystemEvent::SigningHostAccountExhausted { name, period } => self.notice(
                NoticeTone::Warning,
                format!("Account {name} has no free slots; switching accounts"),
                Some(format!("Statement period {period}")),
            ),
            SystemEvent::SigningHostExit { outcome } => self.notice(
                NoticeTone::Info,
                "Pairing responder stopped".to_string(),
                Some(human_identifier(&outcome)),
            ),
            SystemEvent::SigningHostError { reason } => self.activity(
                "pairing".to_string(),
                "Pairing failed".to_string(),
                Some(reason),
                ActivityState::Failed,
            ),
            SystemEvent::SigningHostResponderStarted => self.activity(
                "pairing".to_string(),
                "Waiting for the pairing host".to_string(),
                None,
                ActivityState::Running,
            ),
            SystemEvent::RingInfo {
                ring_index,
                members,
            } => self.notice(
                NoticeTone::Success,
                "LitePeople ring ready".to_string(),
                Some(format!("Ring {ring_index} · {members} members")),
            ),
            SystemEvent::AllowanceChecking { target } => self.start_activity(
                format!("allowance:{target}"),
                format!("Preparing {} access", allowance_name(&target)),
                None,
            ),
            SystemEvent::AllowanceReady {
                target,
                sequence,
                block_hash,
                already_allocated,
            } => self.activity(
                format!("allowance:{target}"),
                format!("{} access ready", allowance_name(&target)),
                Some(if already_allocated {
                    format!("Existing allocation · sequence {sequence}")
                } else {
                    format!(
                        "Sequence {sequence} · block {}",
                        abbreviate(block_hash.as_deref().unwrap_or("<unknown>"), 18)
                    )
                }),
                ActivityState::Succeeded,
            ),
            SystemEvent::NotificationDelivered { id, text, deeplink } => self.notice(
                NoticeTone::Info,
                format!("Notification #{id}"),
                Some(match deeplink {
                    Some(deeplink) => format!("{text}\n{deeplink}"),
                    None => text,
                }),
            ),
            SystemEvent::NotificationScheduled {
                id,
                text,
                scheduled_at,
            } => self.notice(
                NoticeTone::Info,
                format!("Notification #{id} scheduled"),
                Some(format!("{text}\nUnix time {scheduled_at} ms")),
            ),
            SystemEvent::NotificationCancelled { id } => self.notice(
                NoticeTone::Info,
                format!("Notification #{id} cancelled"),
                None,
            ),
            SystemEvent::PairingDeeplink { url } => {
                self.start_activity(
                    "pairing".to_string(),
                    "Pairing link ready".to_string(),
                    Some("Open the dedicated link shown by the pairing host.".to_string()),
                );
                self.notice(NoticeTone::Info, "Pairing link".to_string(), Some(url));
            }
            SystemEvent::PairingAuthenticating => self.activity(
                "pairing".to_string(),
                "Authenticating pairing".to_string(),
                None,
                ActivityState::Running,
            ),
            SystemEvent::PairingConnected { user_id } => self.activity(
                "pairing".to_string(),
                user_id.map_or_else(
                    || "Paired".to_string(),
                    |user_id| format!("Paired with {user_id}"),
                ),
                None,
                ActivityState::Succeeded,
            ),
            SystemEvent::PairingDisconnected => {
                if self.connection != "disconnected" {
                    self.notice(NoticeTone::Info, "Pairing ended".to_string(), None);
                }
            }
            SystemEvent::PairingFailed { reason } => self.activity(
                "pairing".to_string(),
                "Pairing failed".to_string(),
                Some(reason),
                ActivityState::Failed,
            ),
            SystemEvent::ScriptStarted => self.activity(
                "script".to_string(),
                "Script running".to_string(),
                None,
                ActivityState::Running,
            ),
            SystemEvent::ScriptExit { code: 0 } => self.activity(
                "script".to_string(),
                "Script finished".to_string(),
                None,
                ActivityState::Succeeded,
            ),
            SystemEvent::ScriptExit { code } => self.activity(
                "script".to_string(),
                "Script failed".to_string(),
                Some(format!("Exit code {code}")),
                ActivityState::Failed,
            ),
            SystemEvent::SessionStatus {
                name,
                path,
                user_id,
            } => {
                let detail = Some(format!("User {user_id}\nPath {path}"));
                if self.entries.iter().any(|entry| {
                    matches!(
                        entry,
                        FeedItem::Activity {
                            key,
                            state: ActivityState::Running,
                            ..
                        } if key == "session"
                    )
                }) {
                    self.activity(
                        "session".to_string(),
                        format!("Session {name} is active"),
                        detail,
                        ActivityState::Succeeded,
                    );
                } else {
                    self.notice(NoticeTone::Info, format!("Session {name}"), detail);
                }
            }
            SystemEvent::SessionSwitching { from, to } => self.start_activity(
                "session".to_string(),
                format!("Switching from {from} to {to}"),
                None,
            ),
            SystemEvent::SessionCreating { name } => self.activity(
                "session".to_string(),
                format!("Creating session {name}"),
                None,
                ActivityState::Running,
            ),
            SystemEvent::LogLevelChanged { level } => self.notice(
                NoticeTone::Success,
                format!("Log level set to {level}"),
                None,
            ),
            SystemEvent::CopiedTranscript { entries } => self.notice(
                NoticeTone::Success,
                format!("Copied {entries} transcript entries"),
                None,
            ),
        }
    }

    fn handle_sso_event(&mut self, event: SsoEvent) {
        let Some(kind) = event.kind.as_deref() else {
            if let Some(summary) = event.fallback_summary {
                self.notice(NoticeTone::Info, summary, None);
            }
            return;
        };
        let key = event
            .statement_request_id
            .clone()
            .unwrap_or_else(|| format!("sso:{}", self.next_item_id));
        let name = human_identifier(event.request.as_deref().unwrap_or("SSO request"));
        let metadata = sso_metadata(&event);
        let (name, state) = sso_activity_presentation(name, kind, event.outcome.as_deref());
        let updated = if let Some(FeedItem::Request {
            name: current_name,
            state: current_state,
            metadata: current_metadata,
            elapsed_ms,
            reason,
            ..
        }) = self.entries.iter_mut().rev().find(
            |entry| matches!(entry, FeedItem::Request { key: current, .. } if current == &key),
        ) {
            *current_name = name.clone();
            *current_state = state;
            *current_metadata = metadata.clone();
            *elapsed_ms = event.elapsed_ms;
            *reason = event.reason.clone();
            true
        } else {
            false
        };
        if updated {
            self.recalculate_retained();
            return;
        }
        self.push(FeedItem::Request {
            key,
            name,
            state,
            metadata,
            elapsed_ms: event.elapsed_ms,
            reason: event.reason,
        });
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
                    self.notice(
                        NoticeTone::Warning,
                        "A command is already running".to_string(),
                        Some(
                            "Press Ctrl-C to cancel it before submitting another command."
                                .to_string(),
                        ),
                    );
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
            (false, KeyCode::Char('y' | 'Y')) if self.editor.text().is_empty() => {
                self.answer_approval(true);
            }
            (false, KeyCode::Char('n' | 'N')) if self.editor.text().is_empty() => {
                self.answer_approval(false);
            }
            (false, KeyCode::Enter) => {
                let answer = self.editor.text();
                match parse_approval(&answer) {
                    Some(answer) => self.answer_approval(answer),
                    None => {
                        self.editor.clear();
                        self.notice(NoticeTone::Error, "Answer yes or no".to_string(), None);
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
        if let Some(FeedItem::Approval { outcome, .. }) = self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| matches!(entry, FeedItem::Approval { id, .. } if *id == pending.id))
        {
            *outcome = Some(approved);
        }
        self.recalculate_retained();
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
    let height = frame.area().height;
    let show_vertical_padding = height >= 7;
    let show_footer = height >= 3;
    let vertical_padding = u16::from(show_vertical_padding);
    let base_composer_height = 1 + vertical_padding * 2 + u16::from(show_footer);
    let available_completion_height = if height <= 4 {
        0
    } else {
        height
            .saturating_sub(1)
            .saturating_sub(1)
            .saturating_sub(base_composer_height)
            .min(MAX_VISIBLE_COMPLETIONS as u16)
    };
    let completion_range = completion_window(
        completions.len(),
        app.editor.completion_index(),
        usize::from(available_completion_height),
    );
    let completion_height = completion_range.len() as u16;
    let composer_height = base_composer_height.saturating_add(completion_height);
    let areas = Layout::vertical([Constraint::Min(1), Constraint::Length(composer_height)])
        .split(frame.area());
    app.transcript_height = areas[0].height as usize;

    let transcript_lines = app
        .entries
        .iter()
        .flat_map(|item| feed_item_lines(item, usize::from(areas[0].width)))
        .collect::<Vec<_>>();
    let transcript = Paragraph::new(transcript_lines).wrap(Wrap { trim: false });
    let content_height = transcript.line_count(areas[0].width);
    let top = content_height
        .saturating_sub(app.transcript_height)
        .saturating_sub(app.scroll_from_bottom)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(transcript.scroll((top, 0)), areas[0]);

    let surface_area = areas[1];
    let surface_style = input_surface_style();
    let input_surface_area = Rect::new(
        surface_area.x,
        surface_area.y,
        surface_area.width,
        surface_area.height.saturating_sub(u16::from(show_footer)),
    );
    frame.render_widget(Block::default().style(surface_style), input_surface_area);
    let content_padding = if surface_area.width >= 4 {
        COMPOSER_HORIZONTAL_PADDING
    } else {
        0
    };
    let composer_content_area = Rect::new(
        surface_area.x.saturating_add(content_padding),
        surface_area.y,
        surface_area
            .width
            .saturating_sub(content_padding.saturating_mul(2)),
        surface_area.height,
    );

    if completion_height > 0 {
        let selected = app.editor.completion_index();
        let visible_completions = completions
            .iter()
            .skip(completion_range.start)
            .take(completion_range.len())
            .collect::<Vec<_>>();
        let command_column_width = visible_completions
            .iter()
            .map(|completion| text_display_width(&completion.value))
            .max()
            .unwrap_or_default();
        let items = visible_completions
            .into_iter()
            .map(|completion| {
                let mut spans = vec![Span::styled(
                    completion.value.clone(),
                    semantic_style(Color::Cyan).add_modifier(Modifier::BOLD),
                )];
                if composer_content_area.width >= 40 {
                    let padding =
                        command_column_width.saturating_sub(text_display_width(&completion.value));
                    spans.push(Span::raw(" ".repeat(padding.saturating_add(2))));
                    spans.push(Span::styled(
                        completion.description,
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default()
            .with_selected(Some(selected.saturating_sub(completion_range.start)));
        let completion_area = Rect::new(
            composer_content_area.x,
            surface_area.y.saturating_add(vertical_padding),
            composer_content_area.width,
            completion_height,
        );
        frame.render_stateful_widget(
            List::new(items)
                .style(surface_style)
                .highlight_symbol("› ")
                .highlight_spacing(HighlightSpacing::Always)
                .highlight_style(semantic_style(Color::Cyan).add_modifier(Modifier::BOLD)),
            completion_area,
            &mut state,
        );
    }

    let approval = app.pending_approval.is_some();
    let input = app.editor.text();
    let prompt_area = Rect::new(
        composer_content_area.x,
        surface_area
            .bottom()
            .saturating_sub(1 + u16::from(show_footer) + vertical_padding),
        composer_content_area.width,
        1,
    );
    let viewport = input_viewport(
        &input,
        app.editor.cursor(),
        prompt_area.width.saturating_sub(2),
    );
    let (prompt_text, prompt_style) = if input.is_empty() && !approval {
        (
            "Type / for commands".to_string(),
            Style::default().add_modifier(Modifier::DIM),
        )
    } else {
        (
            viewport.text,
            if approval {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "› ",
                semantic_style(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(prompt_text, prompt_style),
        ]))
        .style(surface_style),
        prompt_area,
    );
    let cursor_x = prompt_area
        .x
        .saturating_add(2)
        .saturating_add(viewport.cursor_x)
        .min(prompt_area.right().saturating_sub(1));
    frame.set_cursor_position((cursor_x, prompt_area.y));

    if show_footer {
        let footer_area = Rect::new(
            surface_area.x,
            surface_area.bottom().saturating_sub(1),
            surface_area.width,
            1,
        );
        frame.render_widget(
            Paragraph::new(composer_status_line(
                app,
                approval,
                completion_height > 0,
                surface_area.width,
            )),
            footer_area,
        );
    }
}

fn composer_status_line(
    app: &App,
    approval: bool,
    autocomplete: bool,
    width: u16,
) -> Line<'static> {
    let mut status = header_line(app, width);
    let hint = footer_text(app, approval, autocomplete, width);
    if hint.is_empty() {
        return status;
    }
    let status_width = status
        .spans
        .iter()
        .map(|span| text_display_width(span.content.as_ref()))
        .sum::<usize>();
    let hint_width = text_display_width(&hint);
    let width = usize::from(width);
    if status_width.saturating_add(hint_width).saturating_add(3) <= width {
        status
            .spans
            .push(Span::raw(" ".repeat(width - status_width - hint_width)));
        status.spans.push(Span::styled(
            hint,
            Style::default().add_modifier(Modifier::DIM),
        ));
        status
    } else {
        status
    }
}

fn footer_text(app: &App, approval: bool, autocomplete: bool, width: u16) -> String {
    if approval {
        return "y approve · n deny · Esc deny".to_string();
    }
    if let Some(command) = app.busy.as_deref() {
        return format!("Running {command} · Ctrl-C cancel");
    }
    if app.scroll_from_bottom > 0 {
        return "Ctrl-D latest · Ctrl-U/D scroll".to_string();
    }
    if autocomplete && width >= 70 {
        return "↑↓ select · Tab/Enter complete".to_string();
    }
    String::new()
}

fn input_surface_style() -> Style {
    if std::env::var_os("NO_COLOR").is_some() {
        Style::default()
    } else {
        let background = detected_terminal_background().unwrap_or((24, 24, 32));
        let (red, green, blue) = blended_surface(background);
        let color = if terminal_supports_true_color() {
            Color::Rgb(red, green, blue)
        } else {
            Color::Indexed(rgb_to_ansi256(red, green, blue))
        };
        Style::default().bg(color)
    }
}

fn detected_terminal_background() -> Option<(u8, u8, u8)> {
    let value = std::env::var("COLORFGBG").ok()?;
    let index = value.rsplit(';').next()?.parse::<u8>().ok()?;
    ansi256_to_rgb(index)
}

fn blended_surface((red, green, blue): (u8, u8, u8)) -> (u8, u8, u8) {
    let luminance =
        (u32::from(red) * 2126 + u32::from(green) * 7152 + u32::from(blue) * 722) / 10_000;
    let (target, percent) = if luminance < 128 {
        (255_u8, 12_u16)
    } else {
        (0_u8, 4_u16)
    };
    let blend = |value: u8| {
        ((u16::from(value) * (100 - percent) + u16::from(target) * percent) / 100) as u8
    };
    (blend(red), blend(green), blend(blue))
}

fn terminal_supports_true_color() -> bool {
    std::env::var("COLORTERM").is_ok_and(|value| {
        value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit")
    })
}

fn ansi256_to_rgb(index: u8) -> Option<(u8, u8, u8)> {
    const BASIC: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (128, 0, 0),
        (0, 128, 0),
        (128, 128, 0),
        (0, 0, 128),
        (128, 0, 128),
        (0, 128, 128),
        (192, 192, 192),
        (128, 128, 128),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (0, 0, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    match index {
        0..=15 => Some(BASIC[usize::from(index)]),
        16..=231 => {
            let index = index - 16;
            let component = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
            Some((
                component(index / 36),
                component((index % 36) / 6),
                component(index % 6),
            ))
        }
        232..=255 => {
            let value = 8 + (index - 232) * 10;
            Some((value, value, value))
        }
    }
}

fn rgb_to_ansi256(red: u8, green: u8, blue: u8) -> u8 {
    let component = |value: u8| ((u16::from(value) * 5 + 127) / 255) as u8;
    16 + 36 * component(red) + 6 * component(green) + component(blue)
}

fn header_line(app: &App, width: u16) -> Line<'static> {
    let user_style = match app.connection.as_str() {
        "failed" => semantic_style(Color::Red),
        "disconnected" | "pairing" | "authenticating" | "connected" => {
            Style::default().add_modifier(Modifier::DIM)
        }
        _ => semantic_style(Color::Green),
    };
    let role = match app.role {
        HostRole::Pairing => "pairing",
        HostRole::Signing => "signing",
    };
    let title = format!(" TrUAPI {role} host");
    let user_prefix = " · 👤 ";
    let network_prefix = " · 🌐 ";
    let product_prefix = " · 📦 ";
    let width = usize::from(width);
    let full_prefix_width = text_display_width(&title)
        .saturating_add(text_display_width(user_prefix))
        .saturating_add(text_display_width(&app.connection))
        .saturating_add(text_display_width(network_prefix))
        .saturating_add(text_display_width(&app.network))
        .saturating_add(text_display_width(product_prefix));

    if full_prefix_width < width {
        let product = ellipsize_display_width(&app.product, width - full_prefix_width);
        return Line::from(vec![
            Span::styled(
                title,
                semantic_style(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(user_prefix),
            Span::styled(app.connection.clone(), user_style),
            Span::raw(network_prefix),
            Span::styled(
                app.network.clone(),
                Style::default().add_modifier(Modifier::DIM),
            ),
            Span::raw(product_prefix),
            Span::styled(product, semantic_style(Color::Cyan)),
        ]);
    }

    let fixed_width = text_display_width(" 👤 ")
        .saturating_add(text_display_width(network_prefix))
        .saturating_add(text_display_width(product_prefix));
    if fixed_width >= width {
        return Line::from(Span::styled(
            ellipsize_display_width(&title, width),
            semantic_style(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    }
    let mut remaining = width - fixed_width;
    let user_budget = text_display_width(&app.connection).min(remaining / 3);
    remaining = remaining.saturating_sub(user_budget);
    let network_budget = text_display_width(&app.network).min(remaining / 2);
    remaining = remaining.saturating_sub(network_budget);
    let product_budget = remaining;

    Line::from(vec![
        Span::raw(" 👤 "),
        Span::styled(
            ellipsize_display_width(&app.connection, user_budget),
            user_style,
        ),
        Span::raw(network_prefix),
        Span::styled(
            ellipsize_display_width(&app.network, network_budget),
            Style::default().add_modifier(Modifier::DIM),
        ),
        Span::raw(product_prefix),
        Span::styled(
            ellipsize_display_width(&app.product, product_budget),
            semantic_style(Color::Cyan),
        ),
    ])
}

fn ellipsize_display_width(value: &str, max_width: usize) -> String {
    if text_display_width(value) <= max_width {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let content_width = max_width - 1;
    let mut result = String::new();
    let mut width = 0usize;
    for character in value.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width.saturating_add(character_width) > content_width {
            break;
        }
        result.push(character);
        width = width.saturating_add(character_width);
    }
    result.push('…');
    result
}

fn semantic_style(color: Color) -> Style {
    if std::env::var_os("NO_COLOR").is_some() {
        Style::default()
    } else {
        Style::default().fg(color)
    }
}

fn completion_window(total: usize, selected: usize, max_visible: usize) -> Range<usize> {
    if max_visible == 0 {
        return 0..0;
    }
    if total <= max_visible {
        return 0..total;
    }
    let start = selected
        .saturating_add(1)
        .saturating_sub(max_visible)
        .min(total - max_visible);
    start..start + max_visible
}

fn feed_item_cost(item: &FeedItem) -> (usize, usize) {
    let lines = feed_item_plain_lines(item);
    let bytes = lines.iter().map(String::len).sum();
    (lines.len().max(1), bytes)
}

fn feed_item_lines(item: &FeedItem, width: usize) -> Vec<Line<'static>> {
    match item {
        FeedItem::Log(text) => text
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default().add_modifier(Modifier::DIM),
                ))
            })
            .collect(),
        FeedItem::Notice {
            tone,
            title,
            detail,
        } => status_lines(*tone, title, detail.as_deref()),
        FeedItem::Command(command) => {
            vec![
                Line::default(),
                command_divider_line(command, width),
                Line::default(),
            ]
        }
        FeedItem::Stream { kind, lines } => lines
            .iter()
            .enumerate()
            .map(|(index, line)| match kind {
                StreamKind::Stdout => Line::from(format!("  {line}")),
                StreamKind::Stderr => Line::from(vec![
                    Span::styled(
                        if index == 0 { "! " } else { "  " },
                        semantic_style(Color::Red),
                    ),
                    Span::raw(line.clone()),
                ]),
            })
            .collect(),
        FeedItem::Activity {
            label,
            detail,
            state,
            ..
        } => activity_lines(*state, label, detail.as_deref()),
        FeedItem::Approval {
            action,
            detail,
            outcome,
            ..
        } => match outcome {
            Some(true) => status_lines(NoticeTone::Success, &format!("Approved {action}"), None),
            Some(false) => status_lines(NoticeTone::Warning, &format!("Rejected {action}"), None),
            None => vec![
                Line::default(),
                Line::from(vec![
                    Span::styled(
                        "! ",
                        semantic_style(Color::Yellow).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "Approval required",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::default(),
                Line::from(format!("  {action}")),
                Line::from(Span::styled(
                    format!("  {detail}"),
                    Style::default().add_modifier(Modifier::DIM),
                )),
                Line::default(),
                Line::from(Span::styled(
                    "  [y] Approve   [n] Reject   Esc deny",
                    semantic_style(Color::Cyan),
                )),
            ],
        },
        FeedItem::Request {
            name,
            state,
            metadata,
            elapsed_ms,
            reason,
            ..
        } => {
            let label =
                elapsed_ms.map_or_else(|| name.clone(), |elapsed| format!("{name} · {elapsed} ms"));
            let detail = match (reason, metadata) {
                (Some(reason), Some(metadata)) => Some(format!("{reason}\n{metadata}")),
                (Some(reason), None) => Some(reason.clone()),
                (None, Some(metadata)) => Some(metadata.clone()),
                (None, None) => None,
            };
            activity_lines(*state, &label, detail.as_deref())
        }
    }
}

fn command_divider_line(command: &str, width: usize) -> Line<'static> {
    let divider_style = Style::default().add_modifier(Modifier::DIM);
    if width == 0 {
        return Line::from(vec![
            Span::styled("─ ", divider_style),
            Span::styled(
                command.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if width <= 3 {
        return Line::from(Span::styled("─".repeat(width), divider_style));
    }
    let title = truncate_display_width(command, width - 3);
    let used = 3 + text_display_width(&title);
    Line::from(vec![
        Span::styled("─ ", divider_style),
        Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(" {}", "─".repeat(width.saturating_sub(used))),
            divider_style,
        ),
    ])
}

fn truncate_display_width(text: &str, max_width: usize) -> String {
    if text_display_width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let content_width = max_width - 1;
    let mut result = String::new();
    let mut width = 0;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + character_width > content_width {
            break;
        }
        result.push(character);
        width += character_width;
    }
    result.push('…');
    result
}

fn status_lines(tone: NoticeTone, title: &str, detail: Option<&str>) -> Vec<Line<'static>> {
    let (symbol, color) = match tone {
        NoticeTone::Info => ("•", Color::Cyan),
        NoticeTone::Success => ("✓", Color::Green),
        NoticeTone::Warning => ("!", Color::Yellow),
        NoticeTone::Error => ("×", Color::Red),
    };
    let mut title_lines = title.lines();
    let first = title_lines.next().unwrap_or_default();
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{symbol} "), semantic_style(color)),
        Span::raw(first.to_string()),
    ])];
    lines.extend(title_lines.map(|line| Line::from(format!("  {line}"))));
    if let Some(detail) = detail {
        lines.extend(detail.lines().map(|line| {
            Line::from(Span::styled(
                format!("  {line}"),
                Style::default().add_modifier(Modifier::DIM),
            ))
        }));
    }
    lines
}

fn activity_lines(state: ActivityState, label: &str, detail: Option<&str>) -> Vec<Line<'static>> {
    let tone = match state {
        ActivityState::Running => NoticeTone::Info,
        ActivityState::Succeeded => NoticeTone::Success,
        ActivityState::Warning => NoticeTone::Warning,
        ActivityState::Failed => NoticeTone::Error,
        ActivityState::Cancelled => NoticeTone::Warning,
    };
    let mut lines = status_lines(tone, label, detail);
    if state == ActivityState::Running
        && let Some(first) = lines.first_mut()
    {
        first.spans[0] = Span::styled("◌ ", semantic_style(Color::Cyan));
    }
    if state == ActivityState::Cancelled
        && let Some(first) = lines.first_mut()
    {
        first.spans[0] = Span::styled("– ", semantic_style(Color::Yellow));
    }
    lines
}

fn feed_item_plain_lines(item: &FeedItem) -> Vec<String> {
    feed_item_lines(item, 0)
        .into_iter()
        .map(|line| {
            let line = line
                .spans
                .into_iter()
                .map(|span| span.content.into_owned())
                .collect::<Vec<_>>()
                .join("");
            redact_pairing_link(&line)
        })
        .collect()
}

struct InputViewport {
    text: String,
    cursor_x: u16,
}

fn input_viewport(input: &str, cursor: usize, max_width: u16) -> InputViewport {
    let characters = input.chars().collect::<Vec<_>>();
    let cursor = cursor.min(characters.len());
    let max_width = usize::from(max_width);
    if max_width == 0 {
        return InputViewport {
            text: String::new(),
            cursor_x: 0,
        };
    }

    let mut start = 0;
    while start < cursor {
        let marker_width = usize::from(start > 0);
        let before_width = display_width(&characters[start..cursor]);
        if marker_width + before_width <= max_width {
            break;
        }
        start += 1;
    }

    let clipped = start > 0;
    let mut text = if clipped {
        "…".to_string()
    } else {
        String::new()
    };
    let mut width = usize::from(clipped);
    let cursor_x = width + display_width(&characters[start..cursor]);
    for character in &characters[start..] {
        let character_width = UnicodeWidthChar::width(*character).unwrap_or(0);
        if width + character_width > max_width {
            break;
        }
        text.push(*character);
        width += character_width;
    }
    InputViewport {
        text,
        cursor_x: cursor_x.min(max_width) as u16,
    }
}

fn display_width(characters: &[char]) -> usize {
    characters
        .iter()
        .map(|character| UnicodeWidthChar::width(*character).unwrap_or(0))
        .sum()
}

fn text_display_width(text: &str) -> usize {
    text.chars()
        .map(|character| UnicodeWidthChar::width(character).unwrap_or(0))
        .sum()
}

fn redact_command(command: &str) -> String {
    if command.trim_start().starts_with("/pair ") {
        "/pair <pairing link>".to_string()
    } else {
        sanitize_terminal_text(command)
    }
}

fn redact_pairing_link(text: &str) -> String {
    let Some(start) = text.find("polkadotapp://pair?") else {
        return text.to_string();
    };
    format!("{}<pairing link>", &text[..start])
}

fn sanitize_terminal_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            match characters.peek() {
                Some('[') => {
                    characters.next();
                    for value in characters.by_ref() {
                        if ('@'..='~').contains(&value) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    characters.next();
                    while let Some(value) = characters.next() {
                        if value == '\u{7}' {
                            break;
                        }
                        if value == '\u{1b}' && characters.peek() == Some(&'\\') {
                            characters.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if character == '\n' || character == '\t' || !character.is_control() {
            result.push(character);
        }
    }
    result
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes.saturating_sub('…'.len_utf8());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn allowance_name(target: &str) -> &'static str {
    match target {
        "wallet-sso" => "Wallet",
        "device" => "Device",
        _ => "Product",
    }
}

fn human_identifier(value: &str) -> String {
    let value = value
        .split("::")
        .last()
        .unwrap_or(value)
        .replace(['_', '-'], " ");
    let mut characters = value.chars();
    characters.next().map_or(value.clone(), |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

fn sso_activity_presentation(
    name: String,
    kind: &str,
    outcome: Option<&str>,
) -> (String, ActivityState) {
    match (kind, outcome) {
        ("request_received", _) => (name, ActivityState::Running),
        ("response_failed", _) => (format!("{name} failed"), ActivityState::Failed),
        ("response_sent", None | Some("ok")) => (name, ActivityState::Succeeded),
        ("response_sent", Some("partial")) => (
            format!("{name} partially completed"),
            ActivityState::Warning,
        ),
        ("response_sent", Some("not_available")) => {
            (format!("{name} unavailable"), ActivityState::Warning)
        }
        ("response_sent", Some("rejected")) => (format!("{name} rejected"), ActivityState::Failed),
        ("response_sent", Some(_)) => (format!("{name} failed"), ActivityState::Failed),
        _ => (name, ActivityState::Running),
    }
}

fn abbreviate(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars || max_chars < 2 {
        return value.to_string();
    }
    let side = (max_chars - 1) / 2;
    let start = value.chars().take(side).collect::<String>();
    let end = value
        .chars()
        .skip(count.saturating_sub(side))
        .collect::<String>();
    format!("{start}…{end}")
}

fn sso_metadata(event: &SsoEvent) -> Option<String> {
    let request = event
        .statement_request_id
        .as_deref()
        .map(|value| format!("request {}", abbreviate(value, 18)));
    let response = event
        .response_message_id
        .as_deref()
        .or(event.remote_message_id.as_deref())
        .map(|value| format!("message {}", abbreviate(value, 18)));
    match (request, response) {
        (Some(request), Some(response)) => Some(format!("{request} · {response}")),
        (Some(request), None) => Some(request),
        (None, Some(response)) => Some(response),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Position;
    use tracing_subscriber::layer::SubscriberExt;

    fn test_app() -> App {
        App::new(
            "testnet".to_string(),
            "playground.dot".to_string(),
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
        app.handle_approval_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

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
            app.push(FeedItem::Log(index.to_string()));
        }
        assert_eq!(app.entries.len(), TRANSCRIPT_LIMIT);
        assert!(matches!(app.entries.front(), Some(FeedItem::Log(text)) if text == "1"));
    }

    #[test]
    fn transcript_copy_uses_natural_grouped_output_and_redacts_deeplinks() {
        let mut app = test_app();
        app.handle_system_event(SystemEvent::SigningHostReady);
        app.push_command("/pair polkadotapp://pair?handshake=secret".to_string());
        app.stream(StreamKind::Stdout, "user id: alice.dot".to_string());

        let transcript = app.transcript_text();
        assert!(transcript.contains("✓ Signing host ready"));
        assert!(transcript.contains("─ /pair <pairing link>"));
        assert!(transcript.contains("  user id: alice.dot"));
        assert!(!transcript.contains("handshake=secret"));
        assert!(!transcript.contains("SCRIPT ·"));
    }

    #[test]
    fn missing_signing_session_explains_how_to_connect_a_user() {
        let mut app = test_app();

        app.handle_system_event(SystemEvent::SigningHostNeedsSession);

        assert_eq!(
            app.transcript_text(),
            "! No connected user\n  Use /session <name> to start a session."
        );
    }

    #[test]
    fn keyed_activity_updates_in_place() {
        let mut app = test_app();
        for attempt in 1..=90 {
            app.activity(
                "pairing".to_string(),
                "Preparing pairing".to_string(),
                Some(format!("Attempt {attempt}/90")),
                ActivityState::Running,
            );
        }
        app.activity(
            "pairing".to_string(),
            "Paired with alice.dot".to_string(),
            None,
            ActivityState::Succeeded,
        );

        assert_eq!(app.entries.len(), 1);
        assert!(app.transcript_text().contains("✓ Paired with alice.dot"));
    }

    #[test]
    fn repeated_activity_keeps_completed_history() {
        let mut app = test_app();
        app.start_activity("pairing".to_string(), "Preparing pairing".to_string(), None);
        app.activity(
            "pairing".to_string(),
            "Paired with alice.dot".to_string(),
            None,
            ActivityState::Succeeded,
        );
        app.start_activity(
            "pairing".to_string(),
            "Preparing another pairing".to_string(),
            None,
        );
        app.activity(
            "pairing".to_string(),
            "Paired with bob.dot".to_string(),
            None,
            ActivityState::Succeeded,
        );

        assert_eq!(app.entries.len(), 2);
        let transcript = app.transcript_text();
        assert!(transcript.contains("✓ Paired with alice.dot"));
        assert!(transcript.contains("✓ Paired with bob.dot"));
    }

    #[test]
    fn failed_operation_finalizes_new_running_activities() {
        let mut app = test_app();
        let checkpoint = app.next_item_id;
        app.activity(
            "signer".to_string(),
            "Setting up signer".to_string(),
            Some("Waiting for identity".to_string()),
            ActivityState::Running,
        );
        app.finish_running_activities_since(
            checkpoint,
            ActivityState::Failed,
            "Stopped after an error",
        );

        assert!(app.transcript_text().contains("× Setting up signer"));
        assert!(app.transcript_text().contains("Stopped after an error"));
        assert!(!app.entries.iter().any(|entry| matches!(
            entry,
            FeedItem::Activity {
                state: ActivityState::Running,
                ..
            }
        )));
    }

    #[test]
    fn script_streams_group_lines_and_preserve_blank_lines() {
        let mut app = test_app();
        app.stream(StreamKind::Stdout, "first".to_string());
        app.stream(StreamKind::Stdout, String::new());
        app.stream(StreamKind::Stdout, "third".to_string());
        app.stream(StreamKind::Stderr, "failed".to_string());

        assert_eq!(app.entries.len(), 2);
        assert!(matches!(
            app.entries.front(),
            Some(FeedItem::Stream { lines, .. }) if lines == &["first", "", "third"]
        ));
        assert_eq!(app.transcript_text(), "  first\n  \n  third\n! failed");
    }

    #[test]
    fn large_script_output_is_chunked_and_bounded() {
        let mut app = test_app();
        for index in 0..(TRANSCRIPT_LINE_LIMIT + 500) {
            app.stream(StreamKind::Stdout, format!("line {index}"));
        }

        assert!(app.retained_lines <= TRANSCRIPT_LINE_LIMIT);
        assert!(app.retained_bytes <= TRANSCRIPT_BYTE_LIMIT);
        assert!(app.entries.iter().all(|entry| match entry {
            FeedItem::Stream { lines, .. } => lines.len() <= STREAM_CHUNK_LINE_LIMIT,
            _ => true,
        }));
        assert!(app.transcript_text().contains("line 10499"));
        assert!(!app.transcript_text().contains("line 0\n"));
    }

    #[test]
    fn autocomplete_shows_all_current_commands_and_scrolls_larger_menus() {
        let mut app = test_app();
        app.editor.set_text("/");
        let completions = app.editor.completions();
        let visible = completion_window(
            completions.len(),
            app.editor.completion_index(),
            MAX_VISIBLE_COMPLETIONS,
        );
        assert!(
            completions[visible]
                .iter()
                .any(|completion| completion.value == "/copy")
        );

        assert_eq!(completion_window(12, 0, 8), 0..8);
        assert_eq!(completion_window(12, 11, 8), 4..12);
        assert_eq!(completion_window(12, 11, 0), 0..0);
    }

    #[test]
    fn autocomplete_descriptions_share_one_column() -> Result<()> {
        let mut app = test_app();
        app.editor.set_text("/");

        let (screen, _) = render_app(&mut app, 80, 16)?;
        let deeplink = screen
            .lines()
            .find(|line| line.contains("answer a Polkadot"))
            .context("render deeplink completion")?;
        let script = screen
            .lines()
            .find(|line| line.contains("edit the last"))
            .context("render script completion")?;
        let clear = screen
            .lines()
            .find(|line| line.contains("clear the visible"))
            .context("render clear completion")?;

        let column = |line: &str, description: &str| {
            line.find(description)
                .map(|index| text_display_width(&line[..index]))
        };
        assert_eq!(column(deeplink, "answer"), column(script, "edit"));
        assert_eq!(column(script, "edit"), column(clear, "clear the visible"));
        Ok(())
    }

    #[test]
    fn session_event_updates_status_state_and_completions() {
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
    fn pairing_host_status_names_the_role_without_product_or_log_noise() -> Result<()> {
        let mut app = App::new_pairing(
            "paseo-next-v2".to_string(),
            "playground.dot".to_string(),
            LogLevel::Info,
        );
        app.editor.set_text("/");

        let (screen, _) = render_app(&mut app, 120, 14)?;
        let status = line_text(header_line(&app, 120));

        assert_eq!(
            status,
            " TrUAPI pairing host · 👤 disconnected · 🌐 paseo-next-v2 · 📦 playground.dot"
        );
        assert!(!screen.contains("log info"));
        assert!(screen.contains("/script"));
        assert!(!screen.contains("/pair"));
        assert!(!screen.contains("/session"));
        Ok(())
    }

    #[test]
    fn signing_host_status_shows_user_network_and_product_without_labels() -> Result<()> {
        let mut app = test_app();
        app.connection = "alice.dot".to_string();

        let (screen, _) = render_app(&mut app, 120, 8)?;
        let status = line_text(header_line(&app, 120));

        assert_eq!(
            status,
            " TrUAPI signing host · 👤 alice.dot · 🌐 testnet · 📦 playground.dot"
        );
        assert!(!screen.contains("session default"));
        assert!(!screen.contains("log info"));
        Ok(())
    }

    #[test]
    fn long_product_name_is_ellipsized_in_the_status_bar() -> Result<()> {
        let mut app = test_app();
        app.product = "product-name-that-is-far-too-long-for-the-status-bar".to_string();

        let status = line_text(header_line(&app, 80));

        assert!(status.contains('…'));
        assert!(!status.contains(&app.product));
        assert!(text_display_width(&status) <= 80);
        for width in 1..=120 {
            let status = line_text(header_line(&app, width));
            assert!(text_display_width(&status) <= usize::from(width));
        }
        Ok(())
    }

    #[test]
    fn idle_command_guidance_is_an_input_placeholder() -> Result<()> {
        let mut app = test_app();

        let (idle, _) = render_app(&mut app, 80, 7)?;
        assert!(idle.contains("› Type / for commands"));
        assert!(!idle.contains("↑↓ history"));

        app.editor.set_text("/session");
        let (editing, _) = render_app(&mut app, 80, 7)?;
        assert!(!editing.contains("Type / for commands"));
        Ok(())
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
                cli_summary = "SSO response sent · get_account_alias · ok",
                cli_event = "response_sent",
                request = "get_account_alias",
                statement_request_id = "req:1",
                response_message_id = "msg:2",
                outcome = "ok",
                elapsed_ms = 84_u64,
            );
        });
        *active_ui().lock().expect("unlock active test UI") = None;

        let UiEvent::Sso(event) = receiver.try_recv().expect("summary transcript event") else {
            panic!("expected SSO transcript event");
        };
        assert_eq!(event.kind.as_deref(), Some("response_sent"));
        assert_eq!(event.request.as_deref(), Some("get_account_alias"));
        assert_eq!(event.elapsed_ms, Some(84));
    }

    #[test]
    fn system_events_share_copy_between_streaming_and_tui_renderers() {
        let event = SystemEvent::FramesListening {
            url: "ws://127.0.0.1:9956".to_string(),
        };
        assert_eq!(
            event.human(),
            "• Listening for product frames\n  ws://127.0.0.1:9956"
        );

        let mut app = test_app();
        app.handle_system_event(event);
        assert_eq!(
            app.transcript_text(),
            "• Listening for product frames\n  ws://127.0.0.1:9956"
        );
    }

    #[test]
    fn streaming_pairing_event_keeps_the_actionable_link() {
        let event = SystemEvent::PairingDeeplink {
            url: "polkadotapp://pair?handshake=0123".to_string(),
        };

        assert!(event.human().contains("polkadotapp://pair?handshake=0123"));
    }

    #[test]
    fn operator_login_copies_only_the_typed_pairing_deeplink_event() {
        let event = UiEvent::System(SystemEvent::PairingDeeplink {
            url: "polkadotapp://pair?handshake=0123".to_string(),
        });

        assert_eq!(
            pairing_deeplink_to_copy(true, &event),
            Some("polkadotapp://pair?handshake=0123")
        );
        assert_eq!(pairing_deeplink_to_copy(false, &event), None);
        assert_eq!(
            pairing_deeplink_to_copy(true, &UiEvent::Connection("pairing".to_string())),
            None
        );
    }

    #[test]
    fn prompt_cursor_is_after_ascii_and_wide_input() -> Result<()> {
        let mut app = test_app();
        app.editor.set_text("/d");
        let (screen, cursor) = render_app(&mut app, 80, 12)?;
        assert!(
            screen
                .lines()
                .find(|line| line.contains("/d"))
                .is_some_and(|line| line.starts_with(" › /d"))
        );
        assert_eq!(cursor.x, 5);

        app.editor.set_text("/界");
        let (_, cursor) = render_app(&mut app, 80, 12)?;
        assert_eq!(cursor.x, 6);
        Ok(())
    }

    #[test]
    fn long_input_viewport_keeps_cursor_visible() {
        let viewport = input_viewport("/script a/very/long/path.ts", 27, 10);
        assert!(viewport.text.starts_with('…'));
        assert!(viewport.cursor_x <= 10);
        assert!(viewport.text.ends_with("path.ts"));
    }

    #[test]
    fn rendered_command_and_script_output_use_a_quiet_transcript() -> Result<()> {
        let mut app = test_app();
        app.push_command("/script demo.ts".to_string());
        app.handle_system_event(SystemEvent::ScriptStarted);
        app.stream(StreamKind::Stdout, "head follow event {".to_string());
        app.stream(StreamKind::Stdout, "  tag: Initialized".to_string());
        app.handle_system_event(SystemEvent::ScriptExit { code: 0 });

        let (screen, _) = render_app(&mut app, 80, 14)?;
        let divider = screen
            .lines()
            .find(|line| line.contains("/script demo.ts"))
            .expect("render command divider");
        assert!(divider.starts_with("─ /script demo.ts "));
        assert!(divider.ends_with('─'));
        assert!(screen.contains("  head follow event {"));
        assert!(screen.contains("✓ Script finished"));
        assert!(!screen.contains("SCRIPT ·"));
        assert!(!screen.contains("┌ command"));
        Ok(())
    }

    #[test]
    fn script_lifecycle_replaces_running_label_with_outcome() -> Result<()> {
        let mut app = test_app();
        app.handle_system_event(SystemEvent::ScriptStarted);

        let (running, _) = render_app(&mut app, 80, 8)?;
        assert!(running.contains("◌ Script running"));

        app.handle_system_event(SystemEvent::ScriptExit { code: 0 });
        let (finished, _) = render_app(&mut app, 80, 8)?;
        assert!(finished.contains("✓ Script finished"));
        assert!(!finished.contains("Script running"));
        Ok(())
    }

    #[test]
    fn renderer_stays_usable_at_narrow_normal_and_wide_sizes() -> Result<()> {
        for width in [40, 80, 120] {
            let mut app = test_app();
            app.push_command("/script scripts/a-long-product-script.ts".to_string());
            app.stream(
                StreamKind::Stdout,
                "finalized block 0x44119d48ae19d342a58828de9fce45f39bb".to_string(),
            );
            app.editor.set_text("/session production");

            let (screen, cursor) = render_app(&mut app, width, 12)?;

            assert!(screen.contains("👤"));
            assert!(screen.contains("🌐"));
            assert!(screen.contains("📦"));
            if width >= 80 {
                assert!(screen.contains("TrUAPI signing host"));
                assert!(screen.contains("disconnected"));
            }
            assert!(cursor.x < width);
        }
        Ok(())
    }

    #[test]
    fn host_status_is_rendered_below_the_composer() -> Result<()> {
        let mut app = test_app();

        let (short, short_cursor) = render_app(&mut app, 80, 4)?;
        assert_eq!(short_cursor.y, 2);
        assert!(
            short
                .lines()
                .nth(3)
                .is_some_and(|line| line.contains("TrUAPI"))
        );

        let (medium, medium_cursor) = render_app(&mut app, 80, 5)?;
        assert_eq!(medium_cursor.y, 3);
        assert!(
            medium
                .lines()
                .nth(4)
                .is_some_and(|line| line.contains("TrUAPI"))
        );

        let (normal, normal_cursor) = render_app(&mut app, 80, 7)?;
        assert_eq!(normal_cursor.y, 4);
        assert_eq!(normal.lines().nth(5), Some(""));
        assert!(
            normal
                .lines()
                .nth(6)
                .is_some_and(|line| line.contains("TrUAPI"))
        );
        Ok(())
    }

    #[test]
    fn auto_follow_uses_ratatuis_word_wrapping() -> Result<()> {
        let mut app = test_app();
        for index in 0..8 {
            app.stream(
                StreamKind::Stdout,
                format!(
                    "wrapped row {index} has several words that Ratatui moves between visual lines"
                ),
            );
        }
        app.notice(NoticeTone::Success, "Newest result".to_string(), None);

        let (screen, _) = render_app(&mut app, 40, 10)?;
        assert!(screen.contains("Newest result"));
        Ok(())
    }

    #[test]
    fn approval_text_is_sanitized_before_storage() {
        let mut app = test_app();
        let (response, _answer) = oneshot::channel();
        app.handle_event(UiEvent::Approval {
            action: "\u{1b}[31msign request\u{1b}[0m".to_string(),
            detail: "\u{1b}]0;unsafe title\u{7}safe detail".to_string(),
            response,
        });

        let transcript = app.transcript_text();
        assert!(transcript.contains("sign request"));
        assert!(transcript.contains("safe detail"));
        assert!(!transcript.contains('\u{1b}'));
    }

    #[test]
    fn sso_request_and_response_collapse_to_one_human_row() {
        let mut app = test_app();
        app.handle_sso_event(SsoEvent {
            kind: Some("request_received".to_string()),
            request: Some("get_account_alias".to_string()),
            statement_request_id: Some("req:1".to_string()),
            remote_message_id: Some("msg:1".to_string()),
            ..SsoEvent::default()
        });
        app.handle_sso_event(SsoEvent {
            kind: Some("response_sent".to_string()),
            request: Some("get_account_alias".to_string()),
            statement_request_id: Some("req:1".to_string()),
            response_message_id: Some("msg:2".to_string()),
            elapsed_ms: Some(84),
            ..SsoEvent::default()
        });

        assert_eq!(app.entries.len(), 1);
        let transcript = app.transcript_text();
        assert!(transcript.contains("✓ Get account alias · 84 ms"));
        assert!(transcript.contains("request req:1 · message msg:2"));
    }

    #[test]
    fn streaming_sso_summary_keeps_ids_as_one_detail_line() {
        let text = sso_event_text(SsoEvent {
            kind: Some("response_sent".to_string()),
            request: Some("resource_allocation".to_string()),
            statement_request_id: Some("yiBKUPOF".to_string()),
            response_message_id: Some("yiBKUPOF:response".to_string()),
            outcome: Some("ok".to_string()),
            elapsed_ms: Some(901),
            fallback_summary: Some("unused fallback".to_string()),
            ..SsoEvent::default()
        });

        assert_eq!(
            text,
            "✓ Resource allocation · 901 ms\n  request yiBKUPOF · message yiBKUPOF:response"
        );
        assert!(!text.contains("statement_request_id="));
        assert!(!text.contains("responding_to="));
    }

    #[test]
    fn rejected_resource_allocation_is_visibly_failed_with_its_reason() {
        let mut app = test_app();
        app.handle_sso_event(SsoEvent {
            kind: Some("request_received".to_string()),
            request: Some("resource_allocation".to_string()),
            statement_request_id: Some("UaLEyWid".to_string()),
            remote_message_id: Some("UaLEyWid".to_string()),
            ..SsoEvent::default()
        });
        app.handle_sso_event(SsoEvent {
            kind: Some("response_sent".to_string()),
            request: Some("resource_allocation".to_string()),
            statement_request_id: Some("UaLEyWid".to_string()),
            response_message_id: Some("UaLEyWid:response".to_string()),
            outcome: Some("rejected".to_string()),
            elapsed_ms: Some(63_583),
            reason: Some(
                "Requested resource was rejected: timed out waiting for Bulletin authorization"
                    .to_string(),
            ),
            ..SsoEvent::default()
        });

        let transcript = app.transcript_text();
        assert!(transcript.contains("× Resource allocation rejected · 63583 ms"));
        assert!(transcript.contains(
            "Requested resource was rejected: timed out waiting for Bulletin authorization"
        ));
        assert!(transcript.contains("request UaLEyWid · message UaLEyWid:response"));
        assert!(!transcript.contains("✓ Resource allocation"));
    }

    #[test]
    fn partial_and_unavailable_sso_results_use_warning_presentation() {
        assert_eq!(
            sso_activity_presentation(
                "Resource allocation".to_string(),
                "response_sent",
                Some("partial")
            ),
            (
                "Resource allocation partially completed".to_string(),
                ActivityState::Warning
            )
        );
        assert_eq!(
            sso_activity_presentation(
                "Resource allocation".to_string(),
                "response_sent",
                Some("not_available")
            ),
            (
                "Resource allocation unavailable".to_string(),
                ActivityState::Warning
            )
        );
    }

    #[test]
    fn child_output_sanitizer_removes_terminal_control_sequences() {
        assert_eq!(
            sanitize_terminal_text("\u{1b}[31mred\u{1b}[0m\u{1b}]0;title\u{7}"),
            "red"
        );
    }

    #[test]
    fn composer_surface_blends_for_dark_and_light_terminals() {
        assert_eq!(blended_surface((0, 0, 0)), (30, 30, 30));
        assert_eq!(blended_surface((255, 255, 255)), (244, 244, 244));
        assert_eq!(ansi256_to_rgb(16), Some((0, 0, 0)));
        assert_eq!(ansi256_to_rgb(231), Some((255, 255, 255)));
        assert_eq!(rgb_to_ansi256(255, 255, 255), 231);
    }

    fn render_app(app: &mut App, width: u16, height: u16) -> Result<(String, Position)> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|frame| render(frame, app))?;
        let cursor = terminal.backend().cursor_position();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .chunks(usize::from(width))
            .map(|row| {
                row.iter()
                    .map(|cell| cell.symbol())
                    .collect::<Vec<_>>()
                    .join("")
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok((screen, cursor))
    }

    fn line_text(line: Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }
}
