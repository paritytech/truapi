// WebSocket `WireProvider` for @parity/truapi: one binary WS message per
// SCALE protocol frame, sends buffered until the socket opens.
import type { WireProvider } from "../../../../js/packages/truapi/src/index.ts";

export function wsProvider(url: string): WireProvider & { opened: Promise<void> } {
  const ws = new WebSocket(url);
  ws.binaryType = "arraybuffer";
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();
  const pending: Uint8Array[] = [];
  let open = false;

  let resolveOpened: () => void;
  let rejectOpened: (error: Error) => void;
  const opened = new Promise<void>((resolve, reject) => {
    resolveOpened = resolve;
    rejectOpened = reject;
  });

  ws.addEventListener("open", () => {
    open = true;
    for (const frame of pending.splice(0)) ws.send(frame);
    resolveOpened();
  });
  ws.addEventListener("message", (event) => {
    const bytes = new Uint8Array(event.data as ArrayBuffer);
    for (const listener of listeners) listener(bytes);
  });
  ws.addEventListener("close", () => {
    const error = new Error("websocket closed");
    for (const listener of closeListeners) listener(error);
  });
  ws.addEventListener("error", () => {
    const error = new Error("websocket error");
    rejectOpened(error);
    for (const listener of closeListeners) listener(error);
  });

  return {
    opened,
    postMessage(message: Uint8Array) {
      if (open) ws.send(message);
      else pending.push(message);
    },
    subscribe(cb: (message: Uint8Array) => void) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    subscribeClose(cb: (error: Error) => void) {
      closeListeners.add(cb);
      return () => closeListeners.delete(cb);
    },
    dispose() {
      ws.close();
    },
  };
}
