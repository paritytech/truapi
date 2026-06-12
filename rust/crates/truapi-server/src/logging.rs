//! Level-controlled `tracing` output, routed to the host console.
//!
//! Events emitted via the `tracing` macros (`info!`, `debug!`, …) and
//! `#[instrument]` spans flow through a single subscriber installed once by
//! [`init`]. A reloadable [`LevelFilter`] decides what reaches the console, so
//! the verbosity is tunable at runtime via [`set_level`] (exposed to JS as
//! `setLogLevel`). Disabled by default ([`LevelFilter::OFF`]).
//!
//! On wasm each level maps to the matching `console` method
//! (`error`/`warn`/`info`/`debug`); on native everything goes to stderr.
//! In Chrome, `debug`/`trace` land on `console.debug`, which the DevTools
//! console hides unless its level dropdown includes "Verbose".
//!
//! Output is plaintext, so never log secret material (key bytes, session
//! tokens, signatures).

use std::fmt::{self, Write as _};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Level, Subscriber};
use tracing_subscriber::Registry;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::reload;

static RELOAD_HANDLE: OnceLock<reload::Handle<LevelFilter, Registry>> = OnceLock::new();
static TRACE_SPANS: AtomicBool = AtomicBool::new(false);

/// Install the global subscriber. Idempotent: the first call wins, later
/// calls (and a foreign subscriber already being set) are no-ops.
pub fn init() {
    if RELOAD_HANDLE.get().is_some() {
        return;
    }
    let (filter, handle) = reload::Layer::<LevelFilter, Registry>::new(LevelFilter::OFF);
    let subscriber = Registry::default().with(ConsoleLayer.with_filter(filter));
    if tracing::subscriber::set_global_default(subscriber).is_ok() {
        let _ = RELOAD_HANDLE.set(handle);
    }
}

/// Set the live verbosity threshold. No-op until [`init`] has run.
pub fn set_level(level: LevelFilter) {
    TRACE_SPANS.store(level == LevelFilter::TRACE, Ordering::Relaxed);
    if let Some(handle) = RELOAD_HANDLE.get() {
        let _ = handle.reload(level);
    }
}

/// Apply a host-supplied level string, installing the subscriber first so the
/// call works regardless of whether the core has been constructed yet, then
/// emitting a confirmation event so hosts can verify the logging pipeline end
/// to end. The confirmation is logged at `INFO` (mapping to `console.info`,
/// visible without DevTools "Verbose") rather than at the level just set, so it
/// surfaces even when `debug`/`trace` events land on the hidden `console.debug`.
pub fn set_level_from_str(level: &str) {
    init();
    set_level(parse_level(level));
    tracing::info!(level, "log level set");
}

/// Parse a host-supplied level string. Unknown values disable logging.
pub fn parse_level(level: &str) -> LevelFilter {
    match level.to_ascii_lowercase().as_str() {
        "error" => LevelFilter::ERROR,
        "warn" | "warning" => LevelFilter::WARN,
        "info" => LevelFilter::INFO,
        "debug" => LevelFilter::DEBUG,
        "trace" => LevelFilter::TRACE,
        _ => LevelFilter::OFF,
    }
}

/// Routes each event to the console method matching its level.
struct ConsoleLayer;

impl<S> Layer<S> for ConsoleLayer
where
    S: Subscriber,
    S: for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut visitor = EventVisitor::default();
        attrs.record(&mut visitor);
        span.extensions_mut().insert(SpanFields {
            fields: visitor.fields,
        });
        if trace_spans_enabled() {
            emit_span("new", &span);
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut visitor = EventVisitor::default();
        values.record(&mut visitor);
        if visitor.fields.is_empty() {
            return;
        }
        let mut extensions = span.extensions_mut();
        if let Some(fields) = extensions.get_mut::<SpanFields>() {
            if !fields.fields.is_empty() {
                fields.fields.push_str(", ");
            }
            fields.fields.push_str(&visitor.fields);
        } else {
            extensions.insert(SpanFields {
                fields: visitor.fields,
            });
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if !trace_spans_enabled() {
            return;
        }
        let Some(span) = ctx.span(&id) else {
            return;
        };
        emit_span("close", &span);
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let mut line = format!("[truapi] {} {}", meta.level(), meta.target());
        if !visitor.message.is_empty() {
            let _ = write!(line, ": {}", visitor.message);
        }
        if !visitor.fields.is_empty() {
            let _ = write!(line, " {{{}}}", visitor.fields);
        }
        emit(*meta.level(), &line);
    }
}

#[derive(Default)]
struct SpanFields {
    fields: String,
}

fn trace_spans_enabled() -> bool {
    TRACE_SPANS.load(Ordering::Relaxed)
}

fn emit_span<S>(kind: &str, span: &tracing_subscriber::registry::SpanRef<'_, S>)
where
    S: Subscriber,
    S: for<'a> LookupSpan<'a>,
{
    let meta = span.metadata();
    let mut line = format!("[truapi] TRACE {}: span {}", meta.target(), kind);
    let extensions = span.extensions();
    let fields = extensions.get::<SpanFields>();
    let _ = write!(line, " {{span={:?}", meta.name());
    if let Some(fields) = fields
        && !fields.fields.is_empty()
    {
        let _ = write!(line, ", {}", fields.fields);
    }
    line.push('}');
    emit(Level::TRACE, &line);
}

/// Collects the implicit `message` field separately from explicit key-values.
#[derive(Default)]
struct EventVisitor {
    message: String,
    fields: String,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            if !self.fields.is_empty() {
                self.fields.push_str(", ");
            }
            let _ = write!(self.fields, "{}={value:?}", field.name());
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn emit(_level: Level, line: &str) {
    eprintln!("{line}");
}

#[cfg(target_arch = "wasm32")]
fn emit(level: Level, line: &str) {
    let js = wasm_bindgen::JsValue::from_str(line);
    match level {
        Level::ERROR => web_sys::console::error_1(&js),
        Level::WARN => web_sys::console::warn_1(&js),
        Level::INFO => web_sys::console::info_1(&js),
        Level::DEBUG | Level::TRACE => web_sys::console::debug_1(&js),
    }
}
