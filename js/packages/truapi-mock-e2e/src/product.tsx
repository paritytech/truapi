import { createRoot } from "react-dom/client";
import { useEffect, useState } from "react";
// This is the PRODUCT. It uses the SDK's normal sandbox entry — the exact
// production path. `getClientSync()` detects it's inside a host iframe, awaits
// the host's MessagePort, and returns the real client. No mock code here.
import { getClientSync } from "@parity/truapi/sandbox";
import type { HexString } from "@parity/truapi";

const toHex = (s: string): HexString =>
  ("0x" + Array.from(new TextEncoder().encode(s), (b) => b.toString(16).padStart(2, "0")).join("")) as HexString;
const fromHex = (h?: string) =>
  h ? new TextDecoder().decode(Uint8Array.from(h.slice(2).match(/../g)!.map((x) => parseInt(x, 16)))) : "(none)";

type Row = { name: string; result: string; ok: boolean };

function Product() {
  const [status, setStatus] = useState("connecting to host…");
  const [rows, setRows] = useState<Row[]>([]);

  useEffect(() => {
    (async () => {
      const client = getClientSync();
      if (!client) {
        setStatus("NOT inside a host container");
        return;
      }
      const out: Row[] = [];
      const add = (name: string, result: string, ok = true) => {
        out.push({ name, result, ok });
        setRows([...out]);
      };
      setStatus("connected — running product calls…");
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
      (await client.system.navigateTo({ url: "https://polkadot.network" })).match(
        () => add("system.navigateTo", "sent (host records it)"),
        () => add("system.navigateTo", "ERR", false),
      );
      setStatus("done");
    })().catch((e) => setStatus("ERROR: " + (e?.message ?? String(e))));
  }, []);

  return (
    <div style={{ fontFamily: "system-ui, sans-serif", padding: 16 }}>
      <h2 style={{ marginTop: 0 }}>Product (running in local mock mode)</h2>
      <p data-testid="status">
        <b>status:</b> {status}
      </p>
      <table data-testid="results" border={1} cellPadding={6} style={{ borderCollapse: "collapse" }}>
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

createRoot(document.getElementById("root")!).render(<Product />);
