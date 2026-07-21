/**
 * Legacy Nova iframe transport compatibility.
 *
 * Delete this module together with the marked fallback in `sandbox.ts` once
 * every iframe host transfers a `MessagePort` in response to `truapi-ready`.
 * The inbound `host_handshake_request` compatibility handler in `client.ts`
 * can be removed at the same time.
 *
 * @internal
 */

import type { WireProvider } from "./transport.js";

export interface LegacyIframeProvider {
  provider: WireProvider;
  initialMessage: Uint8Array;
}

/**
 * Recognize a legacy host's first raw SCALE frame and create a provider pinned
 * to that frame's parent window and origin. Modern bootstrap messages return
 * `null` without changing any state.
 */
export function tryCreateLegacyIframeProvider(
  win: Window,
  target: Window,
  event: MessageEvent,
): LegacyIframeProvider | null {
  if (!(event.data instanceof Uint8Array)) return null;

  return {
    provider: createLegacyHostProvider(win, target, event.origin),
    initialMessage: event.data,
  };
}

/**
 * Legacy hosts exchange raw SCALE frames directly over `window.postMessage`.
 * Inbound frames are accepted only from `target` at `frameOrigin`; outbound
 * frames are posted back to the same origin (`*` when the origin is opaque).
 */
function createLegacyHostProvider(
  win: Window,
  target: Window,
  frameOrigin: string,
): WireProvider {
  const postOrigin =
    frameOrigin !== "" && frameOrigin !== "null" ? frameOrigin : "*";
  let closedError: Error | null = null;
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();

  const close = (error: Error): void => {
    if (closedError) return;
    closedError = error;
    win.removeEventListener("message", onMessage);
    for (const listener of [...closeListeners]) listener(error);
    listeners.clear();
    closeListeners.clear();
  };
  function onMessage(event: MessageEvent): void {
    if (closedError) return;
    if (event.source !== target || event.origin !== frameOrigin) return;
    if (!(event.data instanceof Uint8Array)) return;
    for (const listener of [...listeners]) listener(event.data);
  }
  win.addEventListener("message", onMessage);

  return {
    postMessage(message) {
      if (closedError) throw closedError;
      try {
        target.postMessage(message, postOrigin);
      } catch (error) {
        const normalized =
          error instanceof Error ? error : new Error(String(error));
        close(normalized);
        throw normalized;
      }
    },
    subscribe(callback) {
      if (closedError) return () => {};
      listeners.add(callback);
      return () => listeners.delete(callback);
    },
    subscribeClose(callback) {
      if (closedError) {
        callback(closedError);
        return () => {};
      }
      closeListeners.add(callback);
      return () => closeListeners.delete(callback);
    },
    dispose() {
      close(new Error("legacy iframe provider disposed"));
    },
  };
}
