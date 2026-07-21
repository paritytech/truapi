// Browserless full-bundle driver: runs the UNCHANGED playground static export
// under happy-dom on the main realm and bridges its MessagePort to the
// headless host's WS frame server. One Uint8Array = one binary WS frame.
// Realm discipline matters: a separate realm breaks the transport's
// `instanceof Uint8Array` guard and frames drop silently.
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  DIAGNOSIS_PATH,
  FAILED_COUNT_SELECTOR,
  REPORT_READY_SELECTOR,
  RUN_ALL_SELECTOR,
} from "../../../../../playground/tests/e2e-headless/targets";

const OUT = resolve(
  process.env.PLAYGROUND_OUT ?? join(import.meta.dir, "../../../../../playground/out"),
);
const FRAME_URL = process.env.TRUAPI_FRAME_URL ?? "ws://127.0.0.1:9955";
const DEADLINE_MS = Number(process.env.FULL_BUNDLE_DEADLINE_MS ?? 1_200_000);

// Serve the export so happy-dom's CSS/font fetches resolve.
const MIME: Record<string, string> = {
  ".html": "text/html", ".js": "text/javascript", ".css": "text/css",
  ".woff2": "font/woff2", ".txt": "text/plain", ".json": "application/json",
};
// Native Response, captured before happy-dom can shadow it: this handler only
// ever runs post-registration (it serves happy-dom's own asset fetches), where
// a bare `Response` resolves to happy-dom's class — Bun.serve rejects that and
// Bun.inspects the object, whose Headers carry the entire Window (~13MB of
// stdout per run, and the asset request aborts mid-response).
const NativeResponse = globalThis.Response;
const server = Bun.serve({
  port: 0,
  async fetch(req) {
    let path = new URL(req.url).pathname;
    if (path === "/") path = "/index.html";
    const file = Bun.file(join(OUT, path));
    if (!(await file.exists())) return new NativeResponse("not found", { status: 404 });
    const ext = path.slice(path.lastIndexOf("."));
    return new NativeResponse(file, { headers: { "content-type": MIME[ext] ?? "application/octet-stream" } });
  },
});

// Native WebSocket, captured before happy-dom can shadow it.
const NativeWebSocket = globalThis.WebSocket;
GlobalRegistrator.register({ url: `http://127.0.0.1:${server.port}${DIAGNOSIS_PATH}` });

// Defensive cap: happy-dom / the bundle can console.error whole objects on
// early hydration hiccups; render each argument compactly and bounded so no
// future path can flood stdout. (The ~13MB dump previously seen here was NOT
// console.error — it was Bun.serve rejecting a shadowed Response, fixed above
// via NativeResponse.)
const rawConsoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  const compact = args.map((a) =>
    typeof a === "object" && a !== null ? String(a) : a,
  );
  rawConsoleError(...compact.map((a) => String(a).slice(0, 2_000)));
};

// Bridge: product MessagePort <-> host WS frame server.
const ws = new NativeWebSocket(FRAME_URL);
ws.binaryType = "arraybuffer";
const channel = new MessageChannel();
const pending: Uint8Array[] = [];
let wsOpen = false;
ws.addEventListener("open", () => {
  wsOpen = true;
  for (const frame of pending.splice(0)) ws.send(frame);
});
ws.addEventListener("message", (event: MessageEvent) => {
  channel.port2.postMessage(new Uint8Array(event.data as ArrayBuffer));
});
ws.addEventListener("close", () => console.log("SHIM_STATUS ws closed"));
ws.addEventListener("error", () => {
  console.log("SHIM_STATUS ws error");
  process.exit(2);
});
channel.port2.onmessage = (event: MessageEvent) => {
  const frame = event.data as Uint8Array;
  if (wsOpen) ws.send(frame);
  else pending.push(frame);
};
channel.port2.start?.();

(window as any).__HOST_WEBVIEW_MARK__ = true;
(window as any).__HOST_API_PORT__ = channel.port1;

// Execute the shipped bundle in this realm: index.html scripts in document
// order, then every hashed chunk not already referenced (eager evaluation
// satisfies dynamic imports with zero network).
const html = readFileSync(join(OUT, "index.html"), "utf8");
document.documentElement.innerHTML = html
  .replace(/^<!DOCTYPE html>/i, "")
  .replace(/^<html[^>]*>|<\/html>$/g, "");
const indirectEval = eval;
const executed = new Set<string>();
for (const script of Array.from(document.querySelectorAll("script"))) {
  const src = script.getAttribute("src");
  if (src) {
    const clean = src.split("?")[0];
    executed.add(clean.replace(/^\//, ""));
    indirectEval(readFileSync(join(OUT, clean), "utf8") + `\n//# sourceURL=${clean}`);
  } else if (script.textContent) {
    indirectEval(script.textContent);
  }
}
const chunkDir = join(OUT, "_next/static/chunks");
for (const name of readdirSync(chunkDir).filter((n) => n.endsWith(".js")).sort()) {
  const rel = `_next/static/chunks/${name}`;
  if (executed.has(rel)) continue;
  indirectEval(readFileSync(join(chunkDir, name), "utf8") + `\n//# sourceURL=/${rel}`);
}

// Let React hydrate, then drive the product's own Diagnosis run.
await new Promise((r) => setTimeout(r, 3_000));
console.log("SHIM_STATUS chip:", document.querySelector(".status__label")?.textContent ?? "n/a");
const runButton = document.querySelector(RUN_ALL_SELECTOR!) as { click(): void } | null;
if (!runButton) {
  console.log("SHIM_RESULT fail (run control not found)");
  process.exit(2);
}
runButton.click();

const deadline = Date.now() + DEADLINE_MS;
let lastSummary = "";
while (Date.now() < deadline) {
  await new Promise((r) => setTimeout(r, 2_000));
  const summary = document.querySelector(FAILED_COUNT_SELECTOR)?.textContent ?? "";
  if (summary && summary !== lastSummary) {
    console.log("SHIM_SUMMARY", summary);
    lastSummary = summary;
  }
  if (document.querySelector(REPORT_READY_SELECTOR)) {
    const failed = Number(/(\d+)\s+failed\b/.exec(lastSummary)?.[1] ?? NaN);
    const ok = failed <= 1; // parity with the known baseline: 43 passed, 1 failed
    console.log(`SHIM_RESULT ${ok ? "ok" : "fail"} (summary: ${lastSummary})`);
    process.exit(ok ? 0 : 1);
  }
}
console.log(`SHIM_RESULT fail (deadline; last summary: ${lastSummary})`);
process.exit(1);
