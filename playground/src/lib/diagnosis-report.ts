import type { ServiceInfo } from "./services";
import type { TestEntry } from "./auto-test";
import { renderDiagnosisMarkdown } from "@/shared/diagnosis";

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
// table: a title carrying the host mode and one row per method in declared
// order. The output is deterministic for a given set of results (no timestamp)
// so re-submitting an unchanged run produces an identical report.
export function renderReportMarkdown(
  services: ServiceInfo[],
  results: Record<string, TestEntry>,
  meta: { mode?: HostMode; dropSuccessDetails?: boolean } = {},
): string {
  const mode = meta.mode ?? detectHostMode();
  const rows = [];
  for (const svc of services) {
    for (const m of svc.methods) {
      const id = `${svc.name}/${m.name}`;
      const entry = results[id];
      const status = entry?.status ?? "idle";
      rows.push({ id, status, details: entry?.output });
    }
  }
  return renderDiagnosisMarkdown(rows, {
    title: `Truapi ${mode} Diagnosis`,
    dropSuccessDetails: meta.dropSuccessDetails,
  });
}

// Repo that receives the pre-filled diagnosis-report issues; the
// diagnosis-report workflow turns each into a PR under diagnosis-reports/.
const REPORT_ISSUE_URL = "https://github.com/paritytech/truapi/issues/new";

// GitHub / browsers reject issue URLs beyond ~8 KB. Cap the whole URL below
// that; a report with many verbose failures can still exceed it even after
// success-row details are dropped.
const MAX_ISSUE_URL_LENGTH = 7000;

/**
 * Pre-filled GitHub issue URL carrying `report` for a given host `mode`. The
 * title format and the report's `## Truapi <mode> Diagnosis` heading are what
 * the workflow parses, so they live here next to `renderReportMarkdown`.
 *
 * If the report is still too large to fit in the URL, fall back to a short
 * placeholder body — the caller copies the full report to the clipboard, so
 * the user pastes it into the issue instead.
 */
export function reportIssueUrl(report: string, mode: HostMode): string {
  const buildUrl = (body: string): string => {
    const params = new URLSearchParams({
      labels: "diagnosis-report",
      title: `Diagnosis report: ${mode}`,
      body,
    });
    return `${REPORT_ISSUE_URL}?${params.toString()}`;
  };

  const full = buildUrl(report);
  if (full.length <= MAX_ISSUE_URL_LENGTH) return full;
  return buildUrl(
    `_Diagnosis report was too large to prefill (${report.length} chars) — ` +
      `it has been copied to your clipboard, please paste it here._`,
  );
}
