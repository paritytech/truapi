// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * In-app mount: render the inspector from a {@link DebugSession} that lives in
 * the SAME app as the host — no server, no dial-out, no relay. A host running in
 * the page (dotli) feeds each tapped frame via {@link InAppDebugger.handleFrame};
 * {@link InAppDebugger.mount} renders them with the same engine, renderer, and
 * type-driven denylist the standalone app uses, payload-blind by default.
 *
 * This is the "host and debugger in the same bits" transport: the frames never
 * leave the app, so each browser tab is its own tenant — nothing to host or
 * scope. Browser-only (uses `document`).
 *
 * @module
 */

import { createDebugSession } from "./session.js";
import type { DebugSession, DebugSessionOptions } from "./session.js";
import { wireTraceToView } from "./trace-view.js";
import { renderTraceDetail } from "./trace-render.js";
import { detectRetryStorms } from "./retry-storm.js";
import { TRACE_DETAIL_CSS } from "./trace-styles.js";

/** A same-app debugger: feed it frames, mount its panel. */
export interface InAppDebugger {
  /** The underlying session — grouped traces, per-frame decode gate. */
  readonly session: DebugSession;
  /**
   * Feed one tapped frame: the raw SCALE `ProtocolMessage` bytes, opaque. `dir`
   * is product-vantage (`out` = left the product), matching the standalone tap.
   */
  handleFrame(channelId: string, dir: "in" | "out", frame: Uint8Array): void;
  /**
   * Render a live, self-contained panel into `el` and keep it refreshed; returns
   * a disposer that tears the panel down. Payload-blind unless the session was
   * created with `decodeValues`.
   */
  mount(el: HTMLElement, options?: { refreshMs?: number }): () => void;
}

/**
 * Create an in-app debugger. Decode stays OFF unless `decodeValues` is set (the
 * reveal gate folds under it exactly as {@link createDebugSession} does), so a
 * bundled mount is payload-blind by default.
 */
export function createInAppDebugger(
  options: DebugSessionOptions = {},
): InAppDebugger {
  const session = createDebugSession(options);
  return {
    session,
    handleFrame(channelId, dir, frame) {
      session.handleEnvelope({ channelId, dir, frame });
    },
    mount(el, mountOptions = {}) {
      const style = document.createElement("style");
      style.textContent = TRACE_DETAIL_CSS;
      const list = document.createElement("div");
      list.className = "td-inapp";
      el.append(style, list);

      let disposed = false;
      const render = (): void => {
        if (disposed) return;
        const traces = session.traceEngine.traces();
        const storms = detectRetryStorms(traces);
        list.innerHTML =
          traces.length === 0
            ? `<div class="td-empty">no frames yet</div>`
            : traces
                .map(
                  (trace) =>
                    `<div class="td-drilldown">${renderTraceDetail(
                      wireTraceToView(
                        trace,
                        session.methodNames,
                        storms.get(trace) ?? [],
                        session.sensitiveIds,
                      ),
                      {
                        offerDecode: session.decodeValues,
                        offerReveal: session.revealSensitive,
                      },
                    )}</div>`,
                )
                .join("");
      };
      render();
      const timer = setInterval(render, mountOptions.refreshMs ?? 1000);
      return () => {
        disposed = true;
        clearInterval(timer);
        style.remove();
        list.remove();
      };
    },
  };
}
