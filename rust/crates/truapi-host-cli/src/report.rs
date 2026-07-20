//! Aggregates a fleet's `HostMetricRecord` JSONL into one comparable report.

use std::collections::BTreeMap;
use std::io::BufRead;

use anyhow::Context as _;
use serde::Serialize;

use crate::metrics::{Category, HostMetricRecord, Outcome};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpStats {
    pub count: usize,
    pub errors: usize,
    pub error_rate: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VuStats {
    pub count: usize,
    pub errors: usize,
    pub error_rate: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub run_id: String,
    pub started: String,
    pub ended: String,
    pub duration_secs: Option<f64>,
    pub records: usize,
    pub vus: usize,
    pub skipped_lines: usize,
    pub ops: BTreeMap<String, OpStats>,
    pub per_vu: BTreeMap<u32, VuStats>,
    pub total: OpStats,
}

pub fn parse_lines<R: BufRead>(reader: R) -> (Vec<HostMetricRecord>, usize) {
    let mut records = Vec::new();
    let mut skipped = 0usize;
    for line in reader.lines() {
        let Ok(line) = line else {
            skipped += 1;
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<HostMetricRecord>(&line) {
            Ok(rec) => records.push(rec),
            Err(_) => skipped += 1,
        }
    }
    (records, skipped)
}

/// Nearest-rank percentile over an ascending-sorted slice.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((q / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn op_stats(latencies: &mut [f64], errors: usize) -> OpStats {
    latencies.sort_by(|a, b| a.total_cmp(b));
    let count = latencies.len();
    OpStats {
        count,
        errors,
        error_rate: if count == 0 {
            0.0
        } else {
            errors as f64 / count as f64
        },
        p50_ms: percentile(latencies, 50.0),
        p95_ms: percentile(latencies, 95.0),
        max_ms: latencies.last().copied().unwrap_or(0.0),
    }
}

pub fn aggregate(records: &[HostMetricRecord], skipped_lines: usize) -> RunReport {
    let mut by_op: BTreeMap<String, (Vec<f64>, usize)> = BTreeMap::new();
    let mut by_vu: BTreeMap<u32, (Vec<f64>, usize)> = BTreeMap::new();
    let mut total: (Vec<f64>, usize) = (Vec::new(), 0);
    let mut run_ids: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut started: Option<&str> = None;
    let mut ended: Option<&str> = None;

    for rec in records {
        let is_error = rec.outcome == Outcome::Error;
        let key = format!("{}/{}", category_key(rec.category), rec.op);
        let op = by_op.entry(key).or_default();
        op.0.push(rec.latency_ms);
        op.1 += is_error as usize;
        let vu = by_vu.entry(rec.vu_index).or_default();
        vu.0.push(rec.latency_ms);
        vu.1 += is_error as usize;
        total.0.push(rec.latency_ms);
        total.1 += is_error as usize;
        run_ids.insert(&rec.run_id);
        if started.is_none_or(|s| rec.ts.as_str() < s) {
            started = Some(&rec.ts);
        }
        if ended.is_none_or(|e| rec.ts.as_str() > e) {
            ended = Some(&rec.ts);
        }
    }

    let started = started.unwrap_or("").to_string();
    let ended = ended.unwrap_or("").to_string();
    let duration_secs = match (
        chrono::DateTime::parse_from_rfc3339(&started),
        chrono::DateTime::parse_from_rfc3339(&ended),
    ) {
        (Ok(a), Ok(b)) => Some((b - a).num_milliseconds() as f64 / 1000.0),
        _ => None,
    };

    RunReport {
        run_id: match run_ids.len() {
            1 => run_ids.first().unwrap().to_string(),
            n => format!("multiple({n})"),
        },
        started,
        ended,
        duration_secs,
        records: records.len(),
        vus: by_vu.len(),
        skipped_lines,
        ops: by_op
            .into_iter()
            .map(|(k, (mut lat, err))| (k, op_stats(&mut lat, err)))
            .collect(),
        per_vu: by_vu
            .into_iter()
            .map(|(vu, (mut lat, err))| {
                let s = op_stats(&mut lat, err);
                (
                    vu,
                    VuStats {
                        count: s.count,
                        errors: s.errors,
                        error_rate: s.error_rate,
                        p95_ms: s.p95_ms,
                    },
                )
            })
            .collect(),
        total: op_stats(&mut total.0, total.1),
    }
}

fn category_key(c: Category) -> &'static str {
    match c {
        Category::Frame => "frame",
        Category::Pairing => "pairing",
        Category::Signing => "signing",
        Category::Subscription => "subscription",
        Category::HostCallback => "host_callback",
        Category::ChainRpc => "chain_rpc",
        Category::Storage => "storage",
        Category::Permission => "permission",
        Category::Memory => "memory",
        Category::Session => "session",
    }
}

#[derive(Debug, Serialize)]
pub struct OpDelta {
    pub current: Option<OpStats>,
    pub baseline: Option<OpStats>,
    pub delta_count: Option<i64>,
    pub delta_error_rate_pts: Option<f64>,
    pub delta_p50_ms: Option<f64>,
    pub delta_p95_ms: Option<f64>,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct CompareReport {
    pub current: RunReport,
    pub baseline: RunReport,
    pub delta: BTreeMap<String, OpDelta>,
}

pub fn compare(current: RunReport, baseline: RunReport) -> CompareReport {
    let keys: std::collections::BTreeSet<String> = current
        .ops
        .keys()
        .chain(baseline.ops.keys())
        .cloned()
        .collect();
    let delta = keys
        .into_iter()
        .map(|key| {
            let cur = current.ops.get(&key).cloned();
            let base = baseline.ops.get(&key).cloned();
            let entry = match (&cur, &base) {
                (Some(c), Some(b)) => OpDelta {
                    delta_count: Some(c.count as i64 - b.count as i64),
                    delta_error_rate_pts: Some((c.error_rate - b.error_rate) * 100.0),
                    delta_p50_ms: Some(c.p50_ms - b.p50_ms),
                    delta_p95_ms: Some(c.p95_ms - b.p95_ms),
                    status: "changed",
                    current: cur.clone(),
                    baseline: base.clone(),
                },
                (Some(_), None) => OpDelta {
                    current: cur.clone(),
                    baseline: None,
                    delta_count: None,
                    delta_error_rate_pts: None,
                    delta_p50_ms: None,
                    delta_p95_ms: None,
                    status: "new",
                },
                (None, Some(_)) => OpDelta {
                    current: None,
                    baseline: base.clone(),
                    delta_count: None,
                    delta_error_rate_pts: None,
                    delta_p50_ms: None,
                    delta_p95_ms: None,
                    status: "gone",
                },
                (None, None) => unreachable!("key came from one of the two maps"),
            };
            (key, entry)
        })
        .collect();
    CompareReport {
        current,
        baseline,
        delta,
    }
}

pub fn render_compare_table(cmp: &CompareReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "current run {} ({} records)  vs  baseline run {} ({} records)",
        cmp.current.run_id, cmp.current.records, cmp.baseline.run_id, cmp.baseline.records
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<44} {:>7} {:>8} {:>9} {:>9} {:>9}  status",
        "category/op", "count", "Δcount", "Δerr pts", "Δp50", "Δp95"
    );
    for (key, d) in &cmp.delta {
        let count = d.current.as_ref().map_or(0, |s| s.count);
        let fmt_i = |v: Option<i64>| v.map_or_else(|| "-".to_string(), |v| format!("{v:+}"));
        let fmt_f = |v: Option<f64>| v.map_or_else(|| "-".to_string(), |v| format!("{v:+.1}"));
        let _ = writeln!(
            out,
            "{:<44} {:>7} {:>8} {:>9} {:>9} {:>9}  {}",
            key,
            count,
            fmt_i(d.delta_count),
            fmt_f(d.delta_error_rate_pts),
            fmt_f(d.delta_p50_ms),
            fmt_f(d.delta_p95_ms),
            d.status
        );
    }
    out
}

pub fn render_table(report: &RunReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "run {}  |  {} -> {}  ({})  |  records {}  |  vus {}  |  skipped lines {}",
        report.run_id,
        report.started,
        report.ended,
        report
            .duration_secs
            .map_or_else(|| "n/a".to_string(), |s| format!("{s:.1}s")),
        report.records,
        report.vus,
        report.skipped_lines,
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<44} {:>7} {:>7} {:>7} {:>9} {:>9} {:>9}",
        "category/op", "count", "errors", "err%", "p50", "p95", "max"
    );
    for (key, s) in &report.ops {
        let _ = writeln!(
            out,
            "{:<44} {:>7} {:>7} {:>6.1}% {:>7.1}ms {:>7.1}ms {:>7.1}ms",
            key,
            s.count,
            s.errors,
            s.error_rate * 100.0,
            s.p50_ms,
            s.p95_ms,
            s.max_ms
        );
    }
    let t = &report.total;
    let _ = writeln!(
        out,
        "{:<44} {:>7} {:>7} {:>6.1}% {:>7.1}ms {:>7.1}ms {:>7.1}ms",
        "TOTAL",
        t.count,
        t.errors,
        t.error_rate * 100.0,
        t.p50_ms,
        t.p95_ms,
        t.max_ms
    );
    let _ = writeln!(out);
    for (vu, s) in &report.per_vu {
        let _ = writeln!(
            out,
            "vu {:<41} {:>7} {:>7} {:>6.1}% {:>19.1}ms",
            vu,
            s.count,
            s.errors,
            s.error_rate * 100.0,
            s.p95_ms
        );
    }
    out
}

pub fn load_report(path: &std::path::Path) -> anyhow::Result<RunReport> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let (records, skipped) = parse_lines(std::io::BufReader::new(file));
    anyhow::ensure!(
        !records.is_empty(),
        "no valid records in {}",
        path.display()
    );
    Ok(aggregate(&records, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(vu: u32, cat: Category, op: &str, ms: f64, outcome: Outcome) -> HostMetricRecord {
        HostMetricRecord {
            ts: "2026-07-20T10:00:00Z".into(),
            run_id: "fleet-1".into(),
            vu_index: vu,
            category: cat,
            op: op.into(),
            latency_ms: ms,
            outcome,
            error_class: None,
        }
    }

    #[test]
    fn parse_skips_malformed_lines_and_counts_them() {
        let input = concat!(
            r#"{"ts":"2026-07-20T10:00:00Z","runId":"r","vuIndex":0,"category":"frame","op":"a","latencyMs":1.0,"outcome":"success"}"#,
            "\n",
            "not json\n",
            r#"{"ts":"2026-07-20T10:00:01Z","runId":"r","vuIndex":1,"category":"frame","op":"a","latencyMs":2.0,"outcome":"success"}"#,
            "\n",
            "{\"truncated\": \n",
        );
        let (records, skipped) = parse_lines(input.as_bytes());
        assert_eq!(records.len(), 2);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn percentiles_single_record() {
        let report = aggregate(&[rec(0, Category::Frame, "a", 7.0, Outcome::Success)], 0);
        let stats = &report.ops["frame/a"];
        assert_eq!(stats.p50_ms, 7.0);
        assert_eq!(stats.p95_ms, 7.0);
        assert_eq!(stats.max_ms, 7.0);
    }

    #[test]
    fn percentiles_even_and_odd_counts() {
        // odd: 1,2,3 -> p50 = ceil(0.5*3)=2nd -> 2.0 ; even: 1,2,3,4 -> p50 = ceil(0.5*4)=2nd -> 2.0, p95 = ceil(0.95*4)=4th -> 4.0
        let odd: Vec<_> = [1.0, 2.0, 3.0]
            .iter()
            .map(|ms| rec(0, Category::Frame, "a", *ms, Outcome::Success))
            .collect();
        assert_eq!(aggregate(&odd, 0).ops["frame/a"].p50_ms, 2.0);
        let even: Vec<_> = [1.0, 2.0, 3.0, 4.0]
            .iter()
            .map(|ms| rec(0, Category::Frame, "a", *ms, Outcome::Success))
            .collect();
        let stats = aggregate(&even, 0).ops["frame/a"].clone();
        assert_eq!(stats.p50_ms, 2.0);
        assert_eq!(stats.p95_ms, 4.0);
    }

    #[test]
    fn error_rate_counts_only_error_outcomes() {
        let records = vec![
            rec(0, Category::Signing, "s", 1.0, Outcome::Success),
            rec(0, Category::Signing, "s", 1.0, Outcome::Error),
            rec(0, Category::Signing, "s", 1.0, Outcome::Skipped),
            rec(0, Category::Signing, "s", 1.0, Outcome::Error),
        ];
        let stats = aggregate(&records, 0).ops["signing/s"].clone();
        assert_eq!(stats.count, 4);
        assert_eq!(stats.errors, 2);
        assert!((stats.error_rate - 0.5).abs() < 1e-9);
    }

    #[test]
    fn header_covers_run_vus_span_and_skipped() {
        let mut records = vec![
            rec(0, Category::Frame, "a", 1.0, Outcome::Success),
            rec(1, Category::Frame, "a", 1.0, Outcome::Success),
            rec(2, Category::Frame, "a", 1.0, Outcome::Success),
        ];
        records[0].ts = "2026-07-20T10:00:00Z".into();
        records[2].ts = "2026-07-20T10:00:30Z".into();
        let report = aggregate(&records, 5);
        assert_eq!(report.run_id, "fleet-1");
        assert_eq!(report.vus, 3);
        assert_eq!(report.records, 3);
        assert_eq!(report.skipped_lines, 5);
        assert_eq!(report.started, "2026-07-20T10:00:00Z");
        assert_eq!(report.ended, "2026-07-20T10:00:30Z");
        assert_eq!(report.duration_secs, Some(30.0));
    }

    #[test]
    fn mixed_run_ids_are_labelled() {
        let mut records = vec![
            rec(0, Category::Frame, "a", 1.0, Outcome::Success),
            rec(0, Category::Frame, "a", 1.0, Outcome::Success),
        ];
        records[1].run_id = "fleet-2".into();
        assert_eq!(aggregate(&records, 0).run_id, "multiple(2)");
    }

    #[test]
    fn table_contains_header_ops_and_totals() {
        let records = vec![
            rec(
                0,
                Category::Signing,
                "signing_sign_raw",
                10.0,
                Outcome::Success,
            ),
            rec(
                1,
                Category::Signing,
                "signing_sign_raw",
                30.0,
                Outcome::Error,
            ),
            rec(0, Category::Storage, "storage_set", 5.0, Outcome::Success),
        ];
        let table = render_table(&aggregate(&records, 1));
        for needle in [
            "run fleet-1",
            "records 3",
            "vus 2",
            "skipped lines 1",
            "signing/signing_sign_raw",
            "storage/storage_set",
            "TOTAL",
            "p50",
            "p95",
            "err%",
            "vu 0",
            "vu 1",
        ] {
            assert!(table.contains(needle), "missing {needle:?} in:\n{table}");
        }
    }

    #[test]
    fn compare_marks_changed_new_and_gone_ops() {
        let current = aggregate(
            &[
                rec(0, Category::Signing, "s", 20.0, Outcome::Error),
                rec(0, Category::Storage, "new_op", 1.0, Outcome::Success),
            ],
            0,
        );
        let baseline = aggregate(
            &[
                rec(0, Category::Signing, "s", 10.0, Outcome::Success),
                rec(0, Category::Frame, "old_op", 1.0, Outcome::Success),
            ],
            0,
        );
        let cmp = compare(current, baseline);
        let s = &cmp.delta["signing/s"];
        assert_eq!(s.status, "changed");
        assert_eq!(s.delta_count, Some(0));
        assert!((s.delta_error_rate_pts.unwrap() - 100.0).abs() < 1e-9);
        assert!((s.delta_p95_ms.unwrap() - 10.0).abs() < 1e-9);
        assert_eq!(cmp.delta["storage/new_op"].status, "new");
        assert_eq!(cmp.delta["frame/old_op"].status, "gone");
    }

    #[test]
    fn compare_table_shows_deltas_and_status() {
        let current = aggregate(&[rec(0, Category::Signing, "s", 20.0, Outcome::Error)], 0);
        let baseline = aggregate(&[rec(0, Category::Signing, "s", 10.0, Outcome::Success)], 0);
        let table = render_compare_table(&compare(current, baseline));
        for needle in ["signing/s", "Δp95", "+10.0", "+100.0", "changed"] {
            assert!(table.contains(needle), "missing {needle:?} in:\n{table}");
        }
    }

    #[test]
    fn load_report_errors_on_missing_and_empty_files() {
        assert!(load_report(std::path::Path::new("/nonexistent/x.jsonl")).is_err());
        let dir = std::env::temp_dir().join("metrics-report-test");
        std::fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("empty.jsonl");
        std::fs::write(&empty, "not json\n").unwrap();
        let err = load_report(&empty).unwrap_err().to_string();
        assert!(err.contains("no valid records"), "got: {err}");
    }

    #[test]
    fn json_output_is_deterministic() {
        let records = vec![
            rec(1, Category::Storage, "b", 2.0, Outcome::Success),
            rec(0, Category::Signing, "a", 1.0, Outcome::Error),
        ];
        let a = serde_json::to_string_pretty(&aggregate(&records, 0)).unwrap();
        let b = serde_json::to_string_pretty(&aggregate(&records, 0)).unwrap();
        assert_eq!(a, b);
        assert!(
            a.find("signing/a").unwrap() < a.find("storage/b").unwrap(),
            "ops must be key-sorted"
        );
    }
}
