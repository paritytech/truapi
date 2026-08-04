// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Canonical styling for the shared drill-down renderer's `td-*` classes
 * ({@link renderTraceDetail} / {@link renderFrameValueDetail}), co-located with
 * the class emitter.
 *
 * These rules are lifted VERBATIM from dotli's debug-panel stylesheet
 * (`hosts/dotli/packages/truapi-debug/src/styles.css`, the drill-down section)
 * so the standalone app and dotli render the frame sequence identically, with
 * zero drift. dotli keeps its own copy for now and converges onto this one once
 * the build-graph seam lets it import `@parity/truapi-debugger`. Keep the two in
 * sync until then; do not hand-edit these rules here.
 *
 * Note the vendored `hosts/dotli` submodule is the stale pre-port copy, so most
 * of these drill-down classes are NOT yet byte-comparable against it - this file
 * is the source of truth for them, and the dotli-community port picks them up at
 * convergence. App-level layout (grid, the summary strip, `--payload-w`, etc.)
 * deliberately lives OUTSIDE this file, as overrides after `TRACE_DETAIL_CSS` in
 * the standalone shell, so it never contaminates the shared rules.
 */

/** Verbatim `td-*` drill-down rules; inline into a `<style>` for the standalone app. */
export const TRACE_DETAIL_CSS = String.raw`
.td-empty {
  padding: 32px 16px;
  text-align: center;
  color: #525252;
}

.td-detail-pre {
  background: #0b0b0b;
  border: 1px solid #1f1f1f;
  border-radius: 3px;
  padding: 8px 10px;
  margin: 0;
  font-family: inherit;
  font-size: 11.5px;
  color: #d4d4d4;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

/* Drill-down: the shared @parity/truapi-debugger frame-sequence renderer,
   mounted at the top of the truapi detail pane. Class names are owned by the
   shared renderer; the standalone app carries the same rules. */
.td-drilldown {
  margin-bottom: 10px;
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 4px;
  overflow: hidden;
}
.td-trace-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  background: rgba(255, 255, 255, 0.03);
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  flex-wrap: wrap;
}
.td-trace-id {
  font-size: 10.5px;
  color: #94a3b8;
  overflow: hidden;
  text-overflow: ellipsis;
}
.td-trace-meta {
  font-size: 10.5px;
  color: #6b7280;
}
.td-trace-badges,
.td-frame-badges {
  display: inline-flex;
  gap: 4px;
}
.td-frames {
  display: flex;
  flex-direction: column;
}
.td-frame {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 8px;
  font-size: 11px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
}
.td-frame:last-child {
  border-bottom: none;
}
.td-frame-dir {
  font-weight: bold;
  width: 10px;
}
.td-dir-out {
  color: #fbbf24;
}
.td-dir-in {
  color: #4ade80;
}
.td-frame-role {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #6b7280;
  min-width: 62px;
}
.td-role-request,
.td-role-start {
  color: #fbbf24;
}
.td-role-response {
  color: #4ade80;
}
.td-role-receive,
.td-role-stop,
.td-role-interrupt {
  color: #c084fc;
}
.td-role-malformed {
  color: #f87171;
}
.td-frame-method {
  color: #e0e0e0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.td-frame-method.anon {
  color: #525252;
  font-style: italic;
}
.td-frame-bytes {
  color: #6b7280;
  font-size: 10.5px;
}
.td-frame-latency {
  color: #4ade80;
  font-size: 10.5px;
}
.td-frame-latency.td-latency-start {
  color: #525252;
}
/* Badges: op-level (header) and per-frame share the palette. */
.td-badge,
.td-frame-badge {
  font-size: 9.5px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  padding: 0 5px;
  border-radius: 3px;
  border: 1px solid transparent;
}
.td-badge-orphaned {
  color: #fbbf24;
  background: rgba(251, 191, 36, 0.12);
  border-color: rgba(251, 191, 36, 0.3);
}
.td-badge-malformed {
  color: #f87171;
  background: rgba(248, 113, 113, 0.12);
  border-color: rgba(248, 113, 113, 0.3);
}
.td-badge-retry-storm {
  color: #fb923c;
  background: rgba(251, 146, 60, 0.12);
  border-color: rgba(251, 146, 60, 0.3);
}
.td-badge-truncated {
  color: #94a3b8;
  background: rgba(148, 163, 184, 0.12);
  border-color: rgba(148, 163, 184, 0.3);
}
/* Level-2 decode affordance (standalone app vantage; dotli keeps bytes off). */
.td-frame-decode-btn {
  font: inherit;
  font-size: 10px;
  color: #94a3b8;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 3px;
  padding: 1px 6px;
  cursor: pointer;
}
.td-frame-decode-btn:hover {
  background: rgba(255, 255, 255, 0.08);
}
.td-redacted {
  color: #f87171;
  font-size: 11px;
}
.td-redacted-tag {
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}
.td-bytes-only {
  color: #6b7280;
  font-size: 11px;
}
`;
