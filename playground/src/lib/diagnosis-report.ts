import type { ServiceInfo } from "./services";
import type { TestEntry, TestStatus } from "./auto-test";

const ICON: Record<TestStatus, string> = {
  pass: "✅",
  fail: "❌",
  skipped: "⏭",
  idle: "·",
  running: "↻",
};

export type HostMode = "Web" | "Desktop" | "Android" | "iOS" | "Unknown";

// Decide which native platform a webview host runs on from the user-agent.
// iPadOS Safari masquerades as macOS, so a touch-capable "Mac" inside a native
// webview is treated as iOS.
function nativePlatform(ua: string): HostMode {
  if (/android/i.test(ua)) return "Android";
  if (/iphone|ipad|ipod/i.test(ua)) return "iOS";
  if (
    /\bMac\b/i.test(ua) &&
    typeof navigator !== "undefined" &&
    navigator.maxTouchPoints > 1
  ) {
    return "iOS";
  }
  return "Desktop";
}

// Identify the host the playground runs inside, used as the report column /
// title. Native hosts (Electron desktop app or a mobile webview that sets
// `__HOST_WEBVIEW_MARK__`) are split by platform into Desktop / Android / iOS;
// dot.li running inside a browser iframe is the Web host.
export function detectHostMode(): HostMode {
  if (typeof window === "undefined") return "Unknown";
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  const w = window as Window & { __HOST_WEBVIEW_MARK__?: boolean };
  if (/electron/i.test(ua) || w.__HOST_WEBVIEW_MARK__) {
    return nativePlatform(ua);
  }
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
