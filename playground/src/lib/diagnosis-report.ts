import type { ServiceInfo } from "./services";
import type { TestEntry, TestStatus } from "./auto-test";

const ICON: Record<TestStatus, string> = {
  pass: "✅",
  fail: "❌",
  skipped: "⏭",
  idle: "·",
  running: "↻",
};

export type HostMode = "Web" | "Desktop" | "Unknown";

// Identify the host the playground runs inside, used as the report column /
// title. The Desktop host is the Electron-based Polkadot app (native webview);
// the Web host is dot.li in a browser iframe.
export function detectHostMode(): HostMode {
  if (typeof window === "undefined") return "Unknown";
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  if (/electron/i.test(ua)) return "Desktop";
  const w = window as Window & { __HOST_WEBVIEW_MARK__?: boolean };
  if (w.__HOST_WEBVIEW_MARK__) return "Desktop";
  try {
    if (window !== window.top) return "Web";
  } catch {
    return "Web";
  }
  return "Unknown";
}

// Render the diagnosis results as a copy-pasteable GitHub-flavoured markdown
// table: a title carrying the host mode, the generation timestamp, and one row
// per method in declared order.
export function renderReportMarkdown(
  services: ServiceInfo[],
  results: Record<string, TestEntry>,
  meta: { mode?: HostMode } = {},
): string {
  const mode = meta.mode ?? detectHostMode();
  const lines: string[] = [];
  lines.push(`## Truapi ${mode} Diagnosis`);
  lines.push(`_Generated: ${new Date().toISOString()}_`);
  lines.push("");
  lines.push("| Method | Status |");
  lines.push("| --- | --- |");
  for (const svc of services) {
    for (const m of svc.methods) {
      const id = `${svc.name}/${m.name}`;
      const status = results[id]?.status ?? "idle";
      lines.push(`| \`${id}\` | ${ICON[status]} |`);
    }
  }
  return lines.join("\n");
}
