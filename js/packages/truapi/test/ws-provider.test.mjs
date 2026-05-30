// Smoke test for `createWebSocketProvider` against a stubbed WebSocket
// constructor. Verifies the open/queue lifecycle, the close fan-out, and
// that double-registration of the same listener is independent (Set semantics).

import assert from "node:assert/strict";

import { createWebSocketProvider } from "../dist/transport.js";

function makeStubWebSocket(opts = {}) {
  // Records what the provider did to its socket so the tests can assert.
  const sent = [];
  let openHandler = null;
  let messageHandler = null;
  let closeHandler = null;
  let errorHandler = null;
  let readyState = 0; // CONNECTING
  let sendThrows = opts.sendThrows ?? false;

  class StubWebSocket {
    static get CONNECTING() {
      return 0;
    }
    static get OPEN() {
      return 1;
    }
    static get CLOSING() {
      return 2;
    }
    static get CLOSED() {
      return 3;
    }

    binaryType = "";

    set onopen(fn) {
      openHandler = fn;
    }
    set onmessage(fn) {
      messageHandler = fn;
    }
    set onclose(fn) {
      closeHandler = fn;
    }
    set onerror(fn) {
      errorHandler = fn;
    }

    get readyState() {
      return readyState;
    }

    send(bytes) {
      if (sendThrows) throw new Error("send failed");
      sent.push(bytes);
    }
    close() {
      readyState = 3;
      if (closeHandler) closeHandler({ code: 1000, reason: "" });
    }
  }

  return {
    StubWebSocket,
    sent,
    open() {
      readyState = 1;
      if (openHandler) openHandler();
    },
    setReadyState(state) {
      readyState = state;
    },
    setSendThrows(value) {
      sendThrows = value;
    },
    deliver(data) {
      if (messageHandler) {
        messageHandler({ data });
      }
    },
    inbound(bytes) {
      if (messageHandler) {
        messageHandler({ data: bytes.buffer });
      }
    },
    triggerClose(code, reason) {
      readyState = 3;
      if (closeHandler) closeHandler({ code, reason });
    },
    triggerError() {
      if (errorHandler) errorHandler();
    },
  };
}

// 1. queues outbound while connecting; flushes on open
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });

  provider.postMessage(new Uint8Array([1, 2, 3]));
  provider.postMessage(new Uint8Array([4, 5]));
  assert.equal(stub.sent.length, 0, "nothing sent while CONNECTING");

  stub.open();
  assert.deepEqual(
    stub.sent.map((b) => Array.from(b)),
    [
      [1, 2, 3],
      [4, 5],
    ],
    "queued frames flush in order on open",
  );
}

// 2. fan-out: every active listener receives every inbound frame
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });
  stub.open();

  const received = [];
  const a = (bytes) => received.push(["a", Array.from(bytes)]);
  const b = (bytes) => received.push(["b", Array.from(bytes)]);
  const unsubA = provider.subscribe(a);
  provider.subscribe(b);

  stub.inbound(new Uint8Array([0xaa]));
  unsubA();
  stub.inbound(new Uint8Array([0xbb]));

  assert.deepEqual(received, [
    ["a", [0xaa]],
    ["b", [0xaa]],
    ["b", [0xbb]],
  ]);
}

// 3. subscribe is set-based: re-registering the same callback is idempotent
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });
  stub.open();

  let count = 0;
  const cb = () => {
    count += 1;
  };
  const unsub1 = provider.subscribe(cb);
  const unsub2 = provider.subscribe(cb);
  stub.inbound(new Uint8Array([1]));
  assert.equal(count, 1, "duplicate registration counts as one listener");

  unsub1();
  unsub2();
  stub.inbound(new Uint8Array([2]));
  assert.equal(count, 1, "unsubscribed listener is silent");
}

// 4. subscribeClose fires once on socket close; late subscribers see the stored error
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });
  stub.open();

  const errors = [];
  provider.subscribeClose((err) => errors.push(err));
  stub.triggerClose(1006, "abnormal");
  assert.equal(errors.length, 1);
  assert.match(errors[0].message, /websocket closed/);

  let late = null;
  provider.subscribeClose((err) => {
    late = err;
  });
  assert.equal(late, errors[0], "late subscriber receives the stored close error");
}

// 5. postMessage after close throws the stored error
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });
  stub.open();
  stub.triggerClose(1000, "");
  assert.throws(
    () => provider.postMessage(new Uint8Array([1])),
    /websocket closed/,
  );
}

// 6. triggerError() surfaces a /websocket error/ through subscribeClose
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });
  stub.open();

  const errors = [];
  provider.subscribeClose((err) => errors.push(err));
  stub.triggerError();
  assert.equal(errors.length, 1);
  assert.match(errors[0].message, /websocket error/);
}

// 7. non-ArrayBuffer inbound payloads are dropped without firing listeners
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });
  stub.open();

  let count = 0;
  provider.subscribe(() => {
    count += 1;
  });
  stub.deliver("not-an-arraybuffer");
  stub.deliver({ some: "object" });
  assert.equal(count, 0, "non-ArrayBuffer frames are ignored");

  // A real ArrayBuffer still flows through.
  stub.inbound(new Uint8Array([7]));
  assert.equal(count, 1, "ArrayBuffer frames still deliver");
}

// 8. postMessage while readyState is CLOSING (2) throws /websocket not open/
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });
  stub.open();
  stub.setReadyState(2); // CLOSING
  assert.throws(
    () => provider.postMessage(new Uint8Array([1])),
    /websocket not open/,
  );
}

// 9. a send that throws during the onopen flush closes the provider
{
  const stub = makeStubWebSocket();
  const provider = createWebSocketProvider("ws://127.0.0.1:0/?t=token", {
    WebSocket: stub.StubWebSocket,
  });

  const errors = [];
  provider.subscribeClose((err) => errors.push(err));

  // Queue a frame while CONNECTING, then make the socket throw on send and
  // open it so the flush hits the failing send path.
  provider.postMessage(new Uint8Array([1, 2, 3]));
  stub.setSendThrows(true);
  stub.open();

  assert.equal(
    errors.length,
    1,
    "flush failure surfaces through subscribeClose",
  );
  assert.match(errors[0].message, /send failed/);
  assert.throws(
    () => provider.postMessage(new Uint8Array([4])),
    /send failed/,
    "provider is closed after the failed flush",
  );
}

console.log("createWebSocketProvider tests passed");
