//! Headless-host metrics: one raw per-operation event, written as JSONL.
//!
//! Raw events only; percentiles and aggregation are computed downstream.
//! Opt-in: without `METRICS_JSONL` set, recording is a no-op and the host
//! behaves exactly as before.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// What kind of host operation a record measures.
///
/// `Frame` is the coarse label for a whole product request frame at the
/// WebSocket boundary; the fine-grained variants below are decoded from the
/// wire id. The `#[allow(dead_code)]` covers variants not yet emitted.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Frame,
    Pairing,
    Signing,
    Subscription,
    HostCallback,
    ChainRpc,
    Storage,
    Permission,
    Memory,
    Session,
}

/// Terminal outcome of a measured operation.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Error,
    Skipped,
}

/// One per-operation host metric event. Serialises to camelCase JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostMetricRecord {
    pub ts: String,
    pub run_id: String,
    pub vu_index: u32,
    pub category: Category,
    pub op: String,
    pub latency_ms: f64,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_class: Option<String>,
}

/// Appends `HostMetricRecord`s as newline-delimited JSON.
struct JsonlSink {
    path: PathBuf,
}

impl JsonlSink {
    fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    fn append(&self, record: &HostMetricRecord) -> std::io::Result<()> {
        let line = serde_json::to_string(record).map_err(std::io::Error::other)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")
    }
}

/// Records per-operation metrics for one run. Cheap to clone through an `Arc`.
///
/// Configured from the environment: `METRICS_JSONL` (output path; unset means
/// recording is disabled), `RUN_ID` (defaults to `local`), `VU_INDEX`
/// (defaults to `0`).
pub struct MetricsRecorder {
    sink: Option<JsonlSink>,
    run_id: String,
    vu_index: u32,
}

impl MetricsRecorder {
    /// Build a recorder from the environment (see the type's env-var list).
    pub fn from_env() -> Arc<Self> {
        let sink = std::env::var("METRICS_JSONL")
            .ok()
            .filter(|p| !p.is_empty())
            .map(JsonlSink::new);
        let run_id = std::env::var("RUN_ID")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "local".to_string());
        let vu_index = std::env::var("VU_INDEX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Arc::new(Self {
            sink,
            run_id,
            vu_index,
        })
    }

    /// Record one operation. No-op when no sink is configured.
    pub fn record(
        &self,
        category: Category,
        op: &str,
        latency_ms: f64,
        outcome: Outcome,
        error_class: Option<String>,
    ) {
        let Some(sink) = &self.sink else {
            return;
        };
        let record = HostMetricRecord {
            ts: chrono::Utc::now().to_rfc3339(),
            run_id: self.run_id.clone(),
            vu_index: self.vu_index,
            category,
            op: op.to_string(),
            latency_ms,
            outcome,
            error_class,
        };
        if let Err(err) = sink.append(&record) {
            tracing::warn!(%err, "failed to append host metric record");
        }
    }
}

/// Op recorded for a frame we could not classify against the wire table.
/// This is an emitted-schema value consumed by downstream ingest.
const UNCLASSIFIED_OP: &str = "product_frame";

/// A decoded inbound frame's metric identity.
pub struct FrameClass {
    pub category: Category,
    pub op: String,
    pub request_id: String,
}

/// Classify a raw inbound frame into `(category, op)` via the wire table.
/// Falls back to `(Frame, "product_frame")` when it does not decode or the id
/// is unknown.
pub fn classify_frame(frame: &[u8]) -> FrameClass {
    use parity_scale_codec::Decode;
    use truapi_server::frame::ProtocolMessage;
    use truapi_server::generated::wire_table::{WIRE_TABLE, WireKind};

    let Ok(message) = ProtocolMessage::decode(&mut &*frame) else {
        return FrameClass {
            category: Category::Frame,
            op: UNCLASSIFIED_OP.to_string(),
            request_id: String::new(),
        };
    };
    let request_id = message.request_id;
    let id = message.payload.id;
    for entry in WIRE_TABLE {
        let (is_subscription, matches) = match &entry.kind {
            WireKind::Request(r) => (false, r.request_id == id || r.response_id == id),
            WireKind::Subscription(s) => (
                true,
                s.start_id == id || s.stop_id == id || s.interrupt_id == id || s.receive_id == id,
            ),
        };
        if matches {
            let category = if is_subscription {
                Category::Subscription
            } else {
                category_for_method(entry.method)
            };
            return FrameClass {
                category,
                op: entry.method.to_string(),
                request_id,
            };
        }
    }
    FrameClass {
        category: Category::Frame,
        op: "product_frame".to_string(),
        request_id,
    }
}

/// Decode a frame emitted back to the product and extract the true operation
/// outcome, keyed by `request_id`. Returns `Some` for response frames; `None`
/// for subscription frames or undecodable frames (whose outcome is left to the
/// dispatch result).
///
/// The response payload's result discriminant (`value[1]`: 0 = Ok, 1 = Err;
/// see truapi-server `frame.rs`) is the true outcome, so a domain error is
/// caught even though `receive_frame` returned Ok.
pub fn response_outcome(frame: &[u8]) -> Option<(String, Outcome)> {
    use parity_scale_codec::Decode;
    use truapi_server::frame::ProtocolMessage;
    use truapi_server::generated::wire_table::{WIRE_TABLE, WireKind};

    let message = ProtocolMessage::decode(&mut &*frame).ok()?;
    let id = message.payload.id;
    for entry in WIRE_TABLE {
        if let WireKind::Request(r) = &entry.kind
            && r.response_id == id
        {
            let outcome = match message.payload.value.get(1) {
                Some(1) => Outcome::Error,
                _ => Outcome::Success,
            };
            return Some((message.request_id, outcome));
        }
    }
    None
}

/// Map a method name to a coarse metric category by its namespace prefix.
/// A few mappings are provisional (e.g. `statement` under Signing) and can be
/// refined later without touching the wire.
fn category_for_method(method: &str) -> Category {
    match method.split('_').next().unwrap_or("") {
        "signing" | "entropy" | "statement" => Category::Signing,
        "chain" => Category::ChainRpc,
        "local" => Category::Storage,
        "permissions" => Category::Permission,
        "account" => Category::Pairing,
        "system" => Category::Session,
        "notifications" | "preimage" | "theme" | "resource" | "coin" | "payment" | "chat" => {
            Category::HostCallback
        }
        _ => Category::Frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_maps_by_namespace() {
        assert!(matches!(
            category_for_method("signing_sign_raw"),
            Category::Signing
        ));
        assert!(matches!(
            category_for_method("chain_call"),
            Category::ChainRpc
        ));
        assert!(matches!(
            category_for_method("local_storage_write"),
            Category::Storage
        ));
        assert!(matches!(
            category_for_method("account_request_login"),
            Category::Pairing
        ));
        assert!(matches!(
            category_for_method("mystery_method"),
            Category::Frame
        ));
    }

    fn sample() -> HostMetricRecord {
        HostMetricRecord {
            ts: "2026-07-07T00:00:00+00:00".into(),
            run_id: "run-1".into(),
            vu_index: 0,
            category: Category::Frame,
            op: "product_frame".into(),
            latency_ms: 12.5,
            outcome: Outcome::Success,
            error_class: None,
        }
    }

    #[test]
    fn serialises_to_camel_case_snake_enums() {
        let v = serde_json::to_value(sample()).unwrap();
        assert_eq!(v["runId"], "run-1");
        assert_eq!(v["vuIndex"], 0);
        assert_eq!(v["category"], "frame");
        assert_eq!(v["outcome"], "success");
        assert_eq!(v["latencyMs"], 12.5);
        assert!(v.get("errorClass").is_none(), "None error_class is skipped");
    }

    #[test]
    fn appends_one_line_per_record() {
        let path = std::env::temp_dir().join("truapi-host-cli-metrics-unit.jsonl");
        let _ = std::fs::remove_file(&path);
        let sink = JsonlSink::new(&path);
        sink.append(&sample()).unwrap();
        sink.append(&sample()).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body.lines().count(), 2);
        let first: serde_json::Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
        assert_eq!(first["op"], "product_frame");
    }

    // Pins the versioned payload layout `response_outcome` reads (`value[1]`).
    // Built with the real core encoders, so it trips if truapi-server changes
    // the response wire shape (see the `TODO(shared-core-wire)` in frame.rs).
    #[test]
    fn response_outcome_reads_versioned_result_discriminant() {
        use parity_scale_codec::Encode;
        use truapi_server::frame::{
            Payload, ProtocolMessage, encode_versioned_err_payload,
            encode_versioned_unit_ok_payload,
        };
        use truapi_server::generated::wire_table::{WIRE_TABLE, WireKind};

        let response_id = WIRE_TABLE
            .iter()
            .find_map(|entry| match &entry.kind {
                WireKind::Request(r) => Some(r.response_id),
                _ => None,
            })
            .expect("wire table has at least one request method");
        let frame = |request_id: &str, value: Vec<u8>| {
            ProtocolMessage {
                request_id: request_id.to_string(),
                payload: Payload {
                    id: response_id,
                    value,
                },
            }
            .encode()
        };

        let (id, outcome) = response_outcome(&frame("req-ok", encode_versioned_unit_ok_payload(1)))
            .expect("response frame is classified");
        assert_eq!(id, "req-ok");
        assert!(matches!(outcome, Outcome::Success));

        let (id, outcome) =
            response_outcome(&frame("req-err", encode_versioned_err_payload(0u32, 1)))
                .expect("response frame is classified");
        assert_eq!(id, "req-err");
        assert!(matches!(outcome, Outcome::Error));
    }

    #[test]
    fn record_round_trips_through_json() {
        let line = r#"{"ts":"2026-07-20T10:00:00Z","runId":"fleet-1","vuIndex":2,"category":"signing","op":"signing_sign_raw","latencyMs":12.5,"outcome":"error","errorClass":"NoAllowance"}"#;
        let rec: HostMetricRecord = serde_json::from_str(line).expect("deserializes");
        assert_eq!(rec.run_id, "fleet-1");
        assert_eq!(rec.vu_index, 2);
        assert_eq!(rec.category, Category::Signing);
        assert_eq!(rec.outcome, Outcome::Error);
        assert_eq!(rec.error_class.as_deref(), Some("NoAllowance"));
        let back = serde_json::to_string(&rec).expect("serializes");
        assert_eq!(
            serde_json::from_str::<HostMetricRecord>(&back).unwrap().op,
            rec.op
        );
    }
}
