import assert from "node:assert/strict";

import {
  createIframeProvider,
  createMessagePortProvider,
} from "../src/transport.ts";

/**
 * Install a minimal stub for the global `window` used by
 * `createIframeProvider`. Returns a dispatch helper and a snapshot of the
 * registered message listeners so individual tests can inspect cleanup.
 **/
function installFakeWindow() {
  const listeners = new Set();
  const prior = globalThis.window;
  globalThis.window = {
    addEventListener(name, cb) {
      if (name === "message") listeners.add(cb);
    },
    removeEventListener(name, cb) {
      if (name === "message") listeners.delete(cb);
    },
  };
  return {
    listeners,
    dispatch(event) {
      for (const cb of [...listeners]) cb(event);
    },
    restore() {
      if (prior === undefined) delete globalThis.window;
      else globalThis.window = prior;
    },
  };
}

// --- iframe: source/origin filter + outbound origin pin ---
{
  const win = installFakeWindow();
  try {
    const sent = [];
    const target = {
      postMessage(msg, origin) {
        sent.push({ msg, origin });
      },
    };
    const provider = createIframeProvider({
      target,
      hostOrigin: "https://host.example",
    });

    const received = [];
    provider.subscribe((m) => received.push(m));

    win.dispatch({
      source: target,
      origin: "https://host.example",
      data: new Uint8Array([1, 2, 3]),
    });
    assert.deepEqual([...received[0]], [1, 2, 3]);

    win.dispatch({
      source: {},
      origin: "https://host.example",
      data: new Uint8Array([9]),
    });
    win.dispatch({
      source: target,
      origin: "https://attacker.example",
      data: new Uint8Array([9]),
    });
    win.dispatch({
      source: target,
      origin: "https://host.example",
      data: "not bytes",
    });
    assert.equal(received.length, 1, "filter must drop bad source/origin/type");

    provider.postMessage(new Uint8Array([7]));
    assert.equal(sent.length, 1);
    assert.equal(sent[0].origin, "https://host.example");
    assert.deepEqual([...sent[0].msg], [7]);

    provider.dispose();
  } finally {
    win.restore();
  }
}

// --- iframe: dispose semantics ---
{
  const win = installFakeWindow();
  try {
    const provider = createIframeProvider({
      target: { postMessage() {} },
      hostOrigin: "https://host.example",
    });

    let closeError = null;
    provider.subscribeClose((e) => (closeError = e));

    assert.ok(win.listeners.size > 0, "window listener registered");
    provider.dispose();
    assert.ok(closeError instanceof Error, "subscribeClose fired on dispose");
    assert.equal(win.listeners.size, 0, "window listener removed on dispose");

    // Idempotent.
    provider.dispose();
    assert.throws(() => provider.postMessage(new Uint8Array([1])));

    // Post-close subscribeClose fires immediately with the stored error.
    let late = null;
    provider.subscribeClose((e) => (late = e));
    assert.ok(late instanceof Error);
  } finally {
    win.restore();
  }
}

// --- message port: pending queue + round trip + post-close subscribeClose ---
{
  const { port1, port2 } = new MessageChannel();
  const provider = createMessagePortProvider(port1);

  provider.postMessage(new Uint8Array([42]));

  const drained = await new Promise((resolve) => {
    port2.onmessage = (e) => resolve(e.data);
    port2.start();
  });
  assert.deepEqual([...drained], [42], "queued message drained after resolve");

  const inboundOnce = new Promise((resolve) => {
    const unsubscribe = provider.subscribe((m) => {
      unsubscribe();
      resolve(m);
    });
  });
  port2.postMessage(new Uint8Array([55]));
  assert.deepEqual([...(await inboundOnce)], [55]);

  provider.dispose();
  let lateClose = null;
  provider.subscribeClose((e) => (lateClose = e));
  assert.ok(lateClose instanceof Error, "post-close subscribeClose fires immediately");
  assert.throws(() => provider.postMessage(new Uint8Array([1])));

  // Free the receiver port so the runtime exits cleanly.
  port2.close();
}

console.log("all provider tests passed");
