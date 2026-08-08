export type DiagnosisStatus = "idle" | "running" | "pass" | "fail" | "skipped";

export interface DiagnosisResult {
  id: string;
  status: DiagnosisStatus;
  details?: string;
}

const ICON: Record<DiagnosisStatus, string> = {
  pass: "✅",
  fail: "❌",
  skipped: "⏭",
  idle: "·",
  running: "↻",
};

/** Render ordered diagnosis results as deterministic GitHub Markdown. */
export function renderDiagnosisMarkdown(
  results: DiagnosisResult[],
  options: { title: string; dropSuccessDetails?: boolean },
): string {
  let pass = 0;
  let fail = 0;
  const rows: string[] = [];

  for (const result of results) {
    const reportStatus = result.status === "skipped" ? "fail" : result.status;
    if (reportStatus === "pass") pass++;
    else if (reportStatus === "fail") fail++;
    const details =
      options.dropSuccessDetails && reportStatus === "pass"
        ? ""
        : escapeTableCell(result.details);
    rows.push(`| \`${result.id}\` | ${ICON[reportStatus]} | ${details} |`);
  }

  return [
    `## ${options.title}`,
    "",
    `**${pass} success · ${fail} failed**`,
    "",
    "| Method | Status | Details |",
    "| --- | --- | --- |",
    ...rows,
  ].join("\n");
}

function escapeTableCell(value: string | undefined): string {
  return value?.replace(/\s+/g, " ").replace(/\|/g, "\\|").trim() ?? "";
}
