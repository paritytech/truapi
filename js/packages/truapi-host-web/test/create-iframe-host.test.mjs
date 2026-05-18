// Verify that `createIframeHost` hands a MessagePort back through `onPort`,
// constructs an iframe with the expected attributes, and posts the
// `truapi-init` handshake to the iframe's contentWindow on load.

import assert from "node:assert/strict";
import test from "node:test";

import { createIframeHost } from "../dist/index.js";

function setupFakeDom() {
  // Track listeners on the synthetic `window` and the iframe so the
  // test can simulate the iframe `load` event after construction.
  const iframeListeners = new Map();
  const windowListeners = new Map();
  const contentPostMessage = test.mock.fn();

  const iframe = {
    style: {},
    setAttribute: test.mock.fn(),
    addEventListener: (name, fn) => {
      iframeListeners.set(name, fn);
    },
    removeEventListener: () => {},
    remove: test.mock.fn(),
    referrerPolicy: "",
    src: "",
    contentWindow: {
      postMessage: contentPostMessage,
    },
  };

  const container = {
    appendChild: test.mock.fn(),
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
    removeEventListener: () => {},
  };

  return { iframe, container, contentPostMessage, iframeListeners };
}

function teardownFakeDom() {
  delete globalThis.document;
  delete globalThis.window;
}

test("createIframeHost hands back a MessagePort and posts truapi-init on load", () => {
  const { iframe, container, contentPostMessage, iframeListeners } =
    setupFakeDom();

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
    assert.equal(iframe.src, "http://localhost:5174/");

    // Simulate the iframe finishing load.
    const onLoad = iframeListeners.get("load");
    assert.ok(onLoad, "load handler must be registered");
    onLoad();

    assert.equal(contentPostMessage.mock.callCount(), 1);
    const [body, origin, transferList] = contentPostMessage.mock.calls[0].arguments;
    assert.deepEqual(body, { type: "truapi-init" });
    assert.equal(origin, "http://localhost:5174");
    assert.equal(transferList.length, 1);

    // Idempotent, a second load event must not send another init.
    onLoad();
    assert.equal(contentPostMessage.mock.callCount(), 1);

    host.dispose();
    assert.equal(iframe.remove.mock.callCount(), 1);
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
