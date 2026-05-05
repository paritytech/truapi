import { str, u8 } from './scale.js';
import { idForTag, tagForId } from './generated/wire-table.js';

/** Tagged payload on the wire. */
export interface Payload {
  tag: string;
  value: unknown;
}

/** Top-level wire message. Wire format:
 *   [requestId: SCALE str][discriminant: u8][payload bytes...]
 * The discriminant maps to method/kind tag via the auto-generated wire table.
 */
export interface ProtocolMessage {
  requestId: string;
  payload: Payload;
}

export type WireMessage = Uint8Array | ProtocolMessage;

export interface CodecAdapter {
  payloadEncoding: 'typed' | 'binary';
  encode(message: ProtocolMessage): WireMessage;
  decode(message: WireMessage): ProtocolMessage;
}

/** Raw message pipe abstraction. */
export interface Provider {
  postMessage(message: WireMessage): void;
  subscribe(callback: (message: WireMessage) => void): () => void;
  subscribeClose?(callback: (error: Error) => void): () => void;
  dispose(): void;
}

export const structuredCloneCodecAdapter: CodecAdapter = {
  payloadEncoding: 'typed',
  encode: (message) => message,
  decode: (message) => message as ProtocolMessage,
};

function concatBytes(parts: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const p of parts) total += p.length;
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

export const byteProtocolCodecAdapter: CodecAdapter = {
  payloadEncoding: 'binary',
  encode(message) {
    if (!(message.payload.value instanceof Uint8Array)) {
      throw new Error('Binary protocol payloads must already be SCALE encoded');
    }
    const id = idForTag(message.payload.tag);
    if (id === undefined) {
      throw new Error(`Unknown wire tag: ${message.payload.tag}`);
    }
    return concatBytes([
      str.enc(message.requestId),
      u8.enc(id),
      message.payload.value,
    ]);
  },
  decode(message) {
    if (!(message instanceof Uint8Array)) {
      return message;
    }
    if (message.length < 1) {
      throw new Error('Wire frame too short: empty buffer');
    }
    let cursor = message;
    const requestIdEnd = scanStrEnd(cursor);
    const requestId = str.dec(cursor.subarray(0, requestIdEnd));
    cursor = cursor.subarray(requestIdEnd);
    if (cursor.length < 1) {
      throw new Error('Wire frame too short: missing discriminant byte');
    }
    const id = cursor[0];
    const tag = tagForId(id);
    if (tag === undefined) {
      throw new Error(`Unknown wire discriminant: ${id}`);
    }
    const value = cursor.subarray(1);
    // Hand the value bytes back as a fresh slice so callers may safely retain
    // it even if the source buffer is reused by the transport.
    const valueCopy = new Uint8Array(value.length);
    valueCopy.set(value);
    return { requestId, payload: { tag, value: valueCopy } };
  },
};

/** Returns the byte offset just past the SCALE-encoded compact-length-prefixed
 * string at the start of `bytes`. Mirrors what `str.dec` consumes. Bounds-checks
 * each mode so a truncated frame errors instead of silently reading `undefined`. */
function scanStrEnd(bytes: Uint8Array): number {
  if (bytes.length < 1) {
    throw new Error('compact-len: empty buffer');
  }
  const first = bytes[0];
  const mode = first & 0b11;
  let lengthLen: number;
  let strLen: number;
  if (mode === 0) {
    lengthLen = 1;
    strLen = first >> 2;
  } else if (mode === 1) {
    if (bytes.length < 2) {
      throw new Error('compact-len: truncated mode-1 prefix');
    }
    lengthLen = 2;
    strLen = ((first >> 2) | (bytes[1] << 6)) & 0x3fff;
  } else if (mode === 2) {
    if (bytes.length < 4) {
      throw new Error('compact-len: truncated mode-2 prefix');
    }
    lengthLen = 4;
    strLen =
      ((first >> 2) |
        (bytes[1] << 6) |
        (bytes[2] << 14) |
        (bytes[3] << 22)) >>>
      0;
  } else {
    // big-int mode: not used for requestId in our protocol
    throw new Error('compact big-int mode not supported in wire envelope');
  }
  const total = lengthLen + strLen;
  if (total > bytes.length) {
    throw new Error('compact-len: declared length exceeds buffer');
  }
  return total;
}

/** Create a provider from a MessagePort (web/electron). */
export function createMessagePortProvider(
  port: MessagePort | Promise<MessagePort>,
): Provider {
  let resolvedPort: MessagePort | null = null;
  let closedError: Error | null = null;
  const pending: WireMessage[] = [];
  const listeners: Array<(message: WireMessage) => void> = [];
  const closeListeners: Array<(error: Error) => void> = [];

  function notifyClose(error: unknown) {
    const nextError = error instanceof Error ? error : new Error(String(error));
    if (closedError) {
      return;
    }

    closedError = nextError;
    pending.length = 0;
    for (const listener of [...closeListeners]) {
      listener(nextError);
    }
  }

  void Promise.resolve(port)
    .then((p) => {
      if (closedError) {
        try {
          p.close();
        } catch {
          // ignore duplicate close during shutdown
        }
        return;
      }

      resolvedPort = p;
      p.onmessage = (event: MessageEvent) => {
        const data = event.data as WireMessage;
        for (const listener of listeners) listener(data);
      };
      if ('onmessageerror' in p) {
        p.onmessageerror = () => {
          notifyClose(new Error('message port closed unexpectedly'));
        };
      }
      if ('addEventListener' in p) {
        p.addEventListener('close', () => {
          notifyClose(new Error('message port closed unexpectedly'));
        });
      }
      p.start();
      for (const msg of pending) p.postMessage(msg);
      pending.length = 0;
    })
    .catch((error: unknown) => {
      notifyClose(error);
    });

  return {
    postMessage(message) {
      if (closedError) {
        throw closedError;
      }

      if (resolvedPort) {
        try {
          resolvedPort.postMessage(message);
        } catch (error) {
          notifyClose(error);
          throw (error instanceof Error ? error : new Error(String(error)));
        }
      } else {
        pending.push(message);
      }
    },
    subscribe(callback) {
      listeners.push(callback);
      return () => {
        const idx = listeners.indexOf(callback);
        if (idx >= 0) listeners.splice(idx, 1);
      };
    },
    subscribeClose(callback) {
      if (closedError) {
        callback(closedError);
        return () => {};
      }

      closeListeners.push(callback);
      return () => {
        const idx = closeListeners.indexOf(callback);
        if (idx >= 0) closeListeners.splice(idx, 1);
      };
    },
    dispose() {
      notifyClose(new Error('message port provider disposed'));
      try {
        resolvedPort?.close();
      } catch {
        // ignore duplicate close during shutdown
      }
      listeners.length = 0;
      closeListeners.length = 0;
    },
  };
}

export interface WebSocketProviderOptions {
  /** Override WebSocket constructor (tests / non-browser runtimes). */
  WebSocket?: typeof WebSocket;
}

/** Create a provider backed by a binary WebSocket (localhost bridge). */
export function createWebSocketProvider(
  url: string,
  options: WebSocketProviderOptions = {},
): Provider {
  const WebSocketCtor = options.WebSocket ?? globalThis.WebSocket;
  if (!WebSocketCtor) {
    throw new Error('WebSocket constructor not available in this environment');
  }

  const socket = new WebSocketCtor(url);
  socket.binaryType = 'arraybuffer';

  let closedError: Error | null = null;
  const pending: Uint8Array[] = [];
  const listeners: Array<(message: WireMessage) => void> = [];
  const closeListeners: Array<(error: Error) => void> = [];

  function notifyClose(error: unknown) {
    const nextError = error instanceof Error ? error : new Error(String(error));
    if (closedError) {
      return;
    }
    closedError = nextError;
    pending.length = 0;
    for (const listener of [...closeListeners]) {
      listener(nextError);
    }
  }

  socket.onopen = () => {
    for (const msg of pending) {
      try {
        socket.send(msg);
      } catch (error) {
        notifyClose(error);
        return;
      }
    }
    pending.length = 0;
  };

  socket.onmessage = (event: MessageEvent) => {
    const data = event.data;
    if (!(data instanceof ArrayBuffer)) {
      return;
    }
    const bytes = new Uint8Array(data);
    for (const listener of listeners) listener(bytes);
  };

  socket.onerror = () => {
    notifyClose(new Error('websocket error'));
  };

  socket.onclose = (event: CloseEvent) => {
    notifyClose(
      new Error(`websocket closed (code=${event.code}, reason=${event.reason || 'unknown'})`),
    );
  };

  return {
    postMessage(message) {
      if (closedError) {
        throw closedError;
      }
      if (!(message instanceof Uint8Array)) {
        throw new Error('WebSocket provider requires a binary codec adapter');
      }
      if (socket.readyState === WebSocketCtor.OPEN) {
        try {
          socket.send(message);
        } catch (error) {
          notifyClose(error);
          throw (error instanceof Error ? error : new Error(String(error)));
        }
      } else if (socket.readyState === WebSocketCtor.CONNECTING) {
        pending.push(message);
      } else {
        throw new Error('websocket not open');
      }
    },
    subscribe(callback) {
      listeners.push(callback);
      return () => {
        const idx = listeners.indexOf(callback);
        if (idx >= 0) listeners.splice(idx, 1);
      };
    },
    subscribeClose(callback) {
      if (closedError) {
        callback(closedError);
        return () => {};
      }
      closeListeners.push(callback);
      return () => {
        const idx = closeListeners.indexOf(callback);
        if (idx >= 0) closeListeners.splice(idx, 1);
      };
    },
    dispose() {
      notifyClose(new Error('websocket provider disposed'));
      try {
        socket.close();
      } catch {
        // ignore duplicate close during shutdown
      }
      listeners.length = 0;
      closeListeners.length = 0;
    },
  };
}
