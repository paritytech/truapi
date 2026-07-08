import { createRoot } from "react-dom/client";
import { useEffect, useState } from "react";
// The REAL truapi-server core, compiled to WASM, runs in this Web Worker:
import HostWorker from "@parity/truapi-host-wasm/worker-runtime?worker";
// One call wires the mock platform seam, boots the real core in the worker, and
// returns the exact product client a product uses in production.
import { createMockClient } from "@parity/truapi-host-wasm/testing";
import type { HexString } from "@parity/truapi";

const toHex = (s: string): HexString =>
  ("0x" + Array.from(new TextEncoder().encode(s), (b) => b.toString(16).padStart(2, "0")).join("")) as HexString;
const fromHex = (h?: string) =>
  h ? new TextDecoder().decode(Uint8Array.from(h.slice(2).match(/../g)!.map((x) => parseInt(x, 16)))) : "(none)";

type Row = { name: string; result: string; ok: boolean };

function App() {
  const [status, setStatus] = useState("booting real WASM core in a Web Worker…");
  const [rows, setRows] = useState<Row[]>([]);

  useEffect(() => {
    let disposed = false;
    let worker: Worker | undefined;
    (async () => {
      // The whole mock-mode setup, in one line: mock host + real core + client.
      worker = new HostWorker();
      const { client, mock } = await createMockClient(worker, { devicePermissions: "allow-all" });
      if (disposed) return;
      setStatus("core ready — running product calls through the real dispatcher…");

      const out: Row[] = [];
      const add = (name: string, result: string, ok = true) => {
        out.push({ name, result, ok });
        setRows([...out]);
      };

      await client.system.handshake();
      await client.localStorage.write({ key: "profile", value: toHex("nidish") });
      (await client.localStorage.read({ key: "profile" })).match(
        (r) => add("localStorage round-trip", `"${fromHex(r.value)}"`),
        (e) => add("localStorage round-trip", "ERR " + JSON.stringify(e), false),
      );
      (await client.permissions.requestDevicePermission("Camera")).match(
        (r) => add("permissions.Camera (allow-all)", `granted=${r.granted}`, r.granted === true),
        () => add("permissions.Camera", "ERR", false),
      );
      (await client.system.featureSupported({ tag: "Chain", value: { genesisHash: ("0x" + "00".repeat(32)) as HexString } })).match(
        (r) => add("system.featureSupported(Chain)", `supported=${r.supported}`),
        () => add("system.featureSupported", "ERR", false),
      );
      await client.system.navigateTo({ url: "https://polkadot.network" });
      add("system.navigateTo → host recorded", JSON.stringify(mock.navigations()));

      setStatus("done");
    })().catch((e) => setStatus("ERROR: " + (e?.message ?? String(e))));
    return () => {
      disposed = true;
      worker?.terminate();
    };
  }, []);

  return (
    <div style={{ fontFamily: "system-ui, sans-serif", padding: 24, color: "#1b1b1f" }}>
      <h1 style={{ color: "#E6007A" }}>TrUAPI mock host — browser E2E</h1>
      <p>
        Real <code>truapi-server</code> WASM core in a Web Worker, wired in one call with{" "}
        <code>createMockClient</code>. Platform seam mocked; the dispatcher runs for real.
      </p>
      <p data-testid="status">
        <b>status:</b> {status}
      </p>
      <table data-testid="results" border={1} cellPadding={8} style={{ borderCollapse: "collapse" }}>
        <thead>
          <tr>
            <th align="left">product call</th>
            <th align="left">result (through the real core)</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.name}>
              <td>{r.name}</td>
              <td data-testid={`row-${r.name}`} style={{ color: r.ok ? "#0a0" : "#c00" }}>
                {r.result}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
