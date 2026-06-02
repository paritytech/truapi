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

// Identify the host the playground runs inside, used as the report column /
// title. The platform comes from the user-agent first: a mobile host (native
// webview or mobile browser) is Android / iOS, an Electron host is Desktop.
// iPadOS Safari masquerades as macOS, so a touch-capable "Mac" is treated as
// iOS. With no platform marker in the UA, a browser iframe is the Web host.
export function detectHostMode(): HostMode {
  if (typeof window === "undefined") return "Unknown";
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  if (/android/i.test(ua)) return "Android";
  if (/iphone|ipad|ipod/i.test(ua)) return "iOS";
  if (
    /\bMac\b/i.test(ua) &&
    typeof navigator !== "undefined" &&
    navigator.maxTouchPoints > 1
  ) {
    return "iOS";
  }
  const w = window as Window & { __HOST_WEBVIEW_MARK__?: boolean };
  if (/electron/i.test(ua) || w.__HOST_WEBVIEW_MARK__) return "Desktop";
  try {
    return window === window.top ? "Unknown" : "Web";
  } catch {
    return "Web";
  }
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
