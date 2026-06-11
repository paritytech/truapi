// Verify that `createIframeHost` hands a MessagePort back through `onPort`,
// constructs an iframe with the expected attributes, and posts the
// `truapi-init` handshake after the iframe reports readiness.

import assert from "node:assert/strict";
import test from "node:test";

import { createIframeHost } from "../dist/web/index.js";

function setupFakeDom() {
  // Track listeners on the synthetic `window` and the iframe so the
  // test can simulate the iframe `load` event after construction.
  const iframeListeners = new Map();
  const windowListeners = new Map();
  const windowRemove = test.mock.fn();
  const contentPostMessage = test.mock.fn();

  const contentWindow = {
    postMessage: contentPostMessage,
  };

  const iframe = {
    style: {},
    setAttribute: test.mock.fn(),
    addEventListener: (name, fn) => {
      iframeListeners.set(name, fn);
    },
    removeEventListener: () => {},
    remove: test.mock.fn(),
    referrerPolicy: "",
    credentialless: false,
    src: "",
    contentWindow,
  };

  const container = {
    appendChild: test.mock.fn(),
  };

  // Spy on both MessageChannel ports so dispose() teardown is observable.
  const port1 = { postMessage: test.mock.fn(), close: test.mock.fn() };
  const port2 = { postMessage: test.mock.fn(), close: test.mock.fn() };
  globalThis.MessageChannel = class {
    constructor() {
      this.port1 = port1;
      this.port2 = port2;
    }
  };

  globalThis.document = {
    createElement: (tag) => {
      assert.equal(tag, "iframe");
      return iframe;
    },
  };
  globalThis.window = {
    location: { href: "http://localhost:5174/" },
    addEventListener: (name, fn) => {
      windowListeners.set(name, fn);
    },
    removeEventListener: windowRemove,
  };

  return {
    iframe,
    container,
    contentPostMessage,
    contentWindow,
    iframeListeners,
    windowListeners,
    windowRemove,
    port1,
    port2,
  };
}

function teardownFakeDom() {
  delete globalThis.document;
  delete globalThis.window;
  delete globalThis.MessageChannel;
}

test("createIframeHost hands back a MessagePort and configures the iframe", () => {
  const {
    iframe,
    container,
    iframeListeners,
    windowRemove,
    port1,
    port2,
  } = setupFakeDom();

  try {
    let receivedPort = null;
    const host = createIframeHost({
      iframeUrl: "http://localhost:5174/",
      container,
      onPort: (port) => {
        receivedPort = port;
      },
    });

    assert.ok(receivedPort, "onPort must fire synchronously");
    assert.equal(typeof receivedPort.postMessage, "function");
    assert.equal(container.appendChild.mock.callCount(), 1);
    assert.equal(host.iframe, iframe);
    assert.equal(iframe.credentialless, true);
    assert.equal(iframe.src, "http://localhost:5174/");
    assert.equal(
      iframeListeners.has("load"),
      false,
      "port transfer waits for explicit iframe readiness",
    );

    host.dispose();
    assert.equal(iframe.remove.mock.callCount(), 1);
    assert.equal(
      windowRemove.mock.callCount(),
      1,
      "dispose removes the window message listener",
    );
    assert.equal(windowRemove.mock.calls[0].arguments[0], "message");
    assert.equal(
      port1.close.mock.callCount(),
      1,
      "host port closed on dispose",
    );
    assert.equal(
      port2.close.mock.callCount(),
      1,
      "product port closed on dispose",
    );
  } finally {
    teardownFakeDom();
  }
});

test("createIframeHost sends truapi-init on a same-origin playground-ready message", () => {
  const { contentPostMessage, windowListeners, contentWindow } = setupFakeDom();

  try {
    createIframeHost({
      iframeUrl: "http://localhost:5174/",
      container: { appendChild: () => {} },
      onPort: () => {},
    });

    const onMessage = windowListeners.get("message");
    assert.ok(onMessage, "window message listener must be registered");

    // Wrong source is dropped.
    onMessage({
      source: { other: true },
      origin: "http://localhost:5174",
      data: { type: "truapi-playground-ready" },
    });
    assert.equal(
      contentPostMessage.mock.callCount(),
      0,
      "wrong source dropped",
    );

    // Wrong origin is dropped.
    onMessage({
      source: contentWindow,
      origin: "http://evil.example",
      data: { type: "truapi-playground-ready" },
    });
    assert.equal(
      contentPostMessage.mock.callCount(),
      0,
      "wrong origin dropped",
    );

    // Correct source + origin triggers the init handshake.
    onMessage({
      source: contentWindow,
      origin: "http://localhost:5174",
      data: { type: "truapi-playground-ready" },
    });
    assert.equal(contentPostMessage.mock.callCount(), 1, "ready triggers init");
    const [body, origin] = contentPostMessage.mock.calls[0].arguments;
    assert.deepEqual(body, { type: "truapi-init" });
    assert.equal(origin, "*");

    // The handshake is idempotent across repeated ready events too.
    onMessage({
      source: contentWindow,
      origin: "http://localhost:5174",
      data: { type: "truapi-playground-ready" },
    });
    assert.equal(contentPostMessage.mock.callCount(), 1, "init sent only once");
  } finally {
    teardownFakeDom();
  }
});

test("createIframeHost accepts playground-ready from a credentialless opaque origin", () => {
  const { contentPostMessage, windowListeners, contentWindow } = setupFakeDom();

  try {
    createIframeHost({
      iframeUrl: "http://localhost:5174/",
      container: { appendChild: () => {} },
      onPort: () => {},
    });

    const onMessage = windowListeners.get("message");
    assert.ok(onMessage, "window message listener must be registered");

    onMessage({
      source: contentWindow,
      origin: "null",
      data: { type: "truapi-playground-ready" },
    });
    assert.equal(
      contentPostMessage.mock.callCount(),
      1,
      "opaque credentialless origin triggers init",
    );
    const [, origin] = contentPostMessage.mock.calls[0].arguments;
    assert.equal(origin, "*");
  } finally {
    teardownFakeDom();
  }
});

test("createIframeHost rejects a mismatched allowedOrigin", () => {
  setupFakeDom();
  try {
    assert.throws(
      () =>
        createIframeHost({
          iframeUrl: "http://localhost:5174/",
          container: { appendChild: () => {} },
          onPort: () => {},
          allowedOrigin: "http://localhost:9999",
        }),
      /origin policy mismatch/,
    );
  } finally {
    teardownFakeDom();
  }
});

test("createIframeHost rejects non-http(s) iframe URLs", () => {
  setupFakeDom();
  try {
    assert.throws(
      () =>
        createIframeHost({
          iframeUrl: "file:///etc/passwd",
          container: { appendChild: () => {} },
          onPort: () => {},
        }),
      /only allows http\(s\)/,
    );
  } finally {
    teardownFakeDom();
  }
});
