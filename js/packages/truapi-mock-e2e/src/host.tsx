import { createRoot } from "react-dom/client";
import { useEffect, useRef, useState } from "react";
// The REAL truapi-server core (WASM) runs in this Worker:
import HostWorker from "@parity/truapi-host-wasm/worker-runtime?worker";
import {
  createMockHost,
  mockRuntimeConfig,
  createWebWorkerPairingHostRuntime,
  createIframeHost,
} from "@parity/truapi-host-wasm/web";
import type {
  IframeHost,
  MockHost,
  WebWorkerHostCallbacks,
  WorkerPairingHostRuntime,
} from "@parity/truapi-host-wasm/web";
import { createMessagePortProvider } from "@parity/truapi";

/** Lift the mock's flat callbacks into the namespaced `WebWorkerHostCallbacks`
 *  shape the worker runtime type expects. Every slot points at the one flat
 *  object; `createWasmRawCallbacks` normalizes it at runtime. */
function namespaceMockCallbacks(flat: MockHost["callbacks"]): WebWorkerHostCallbacks {
  return {
    navigation: flat,
    notifications: flat,
    permissions: flat,
    features: flat,
    productStorage: flat,
    coreStorage: flat,
    chain: flat,
    auth: flat,
    userConfirmation: flat,
    theme: flat,
    preimage: flat,
  };
}

declare global {
  interface Window {
    // Host-side assertion surface for Playwright: reads the mock's recorded oracles.
    __MOCK_HOST__?: {
      navigations: () => string[];
      confirmations: () => string[];
      pushedNotifications: () => unknown[];
      sentRpc: () => string[];
    };
  }
}

function Host() {
  const [status, setStatus] = useState("booting real WASM core in a Web Worker…");
  const frameRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let disposed = false;
    let worker: Worker | undefined;
    let iframeHost: IframeHost | undefined;
    let runtime: WorkerPairingHostRuntime | undefined;
    (async () => {
      // Host side: mock only the platform seam; boot the REAL core in the worker.
      worker = new HostWorker();
      const mock = createMockHost({ devicePermissions: "allow-all" });
      const { productId, ...hostConfig } = mockRuntimeConfig();
      runtime = await createWebWorkerPairingHostRuntime(
        worker,
        namespaceMockCallbacks(mock.callbacks),
        { hostConfig },
      );
      const workerProvider = await runtime.createProvider({ productId });
      if (disposed || !frameRef.current) return;

      window.__MOCK_HOST__ = {
        navigations: () => mock.navigations(),
        confirmations: () => mock.confirmations(),
        pushedNotifications: () => mock.pushedNotifications(),
        sentRpc: () => mock.sentRpc(),
      };

      // Embed the product iframe; bridge its MessagePort <-> the worker core.
      iframeHost = createIframeHost({
        iframeUrl: new URL("/product.html", location.href).toString(),
        container: frameRef.current,
        onPort: (hostPort) => {
          const portProvider = createMessagePortProvider(hostPort);
          // product -> core
          portProvider.subscribe((bytes) => workerProvider.postMessage(bytes));
          // core -> product
          workerProvider.subscribe((bytes) => portProvider.postMessage(bytes));
        },
      });

      setStatus("ready — real core in worker, product embedded in iframe");
    })().catch((e) => setStatus("ERROR: " + (e?.message ?? String(e))));
    return () => {
      disposed = true;
      iframeHost?.dispose();
      runtime?.dispose();
      worker?.terminate();
    };
  }, []);

  return (
    <div style={{ fontFamily: "system-ui, sans-serif", padding: 24, color: "#1b1b1f" }}>
      <h1 style={{ color: "#E6007A", marginBottom: 4 }}>TrUAPI mock host — full browser E2E</h1>
      <p style={{ marginTop: 0 }}>
        Host: <code>createMockHost</code> + real WASM core in a Worker. Product runs in the iframe
        below and talks to this host over a real MessageChannel — nothing hand-rolled.
      </p>
      <p data-testid="host-status">
        <b>host:</b> {status}
      </p>
      <div
        ref={frameRef}
        data-testid="frame"
        style={{ width: "100%", height: 360, border: "2px solid #E6007A", borderRadius: 8 }}
      />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<Host />);
