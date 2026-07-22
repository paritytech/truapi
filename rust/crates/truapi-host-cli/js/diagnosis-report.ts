import type { DiagnosisRow } from "./diagnosis.ts";

/** Render the same Markdown matrix used by the playground diagnosis reports. */
export function renderDiagnosisReport(
  title: string,
  rows: DiagnosisRow[],
): string {
  return (
    [
      `## ${title}`,
      "",
      "| Method | Status | Details |",
      "| --- | --- | --- |",
      ...rows.map(
        (row) =>
          `| \`${row.id}\` | ${statusIcon(row.status)} | ${
            row.status === "pass" ? "" : cleanMarkdownDetail(row.output)
          } |`,
      ),
    ].join("\n") + "\n"
  );
}

function statusIcon(status: DiagnosisRow["status"]): string {
  if (status === "pass") return "✅";
  if (status === "skipped") return "⏭️";
  return "❌";
}

export function cleanMarkdownDetail(output: string): string {
  const collapsed = output
    .replace(/\u001b\[[0-9;]*m/g, "")
    .replace(/\s+/g, " ")
    .trim();
  const concise = truncateMiddle(collapsed, 300);
  return concise.replaceAll("|", "\\|");
}

function truncateMiddle(value: string, limit: number): string {
  if (value.length <= limit) return value;
  const tailLength = Math.ceil(limit * 0.6);
  const headLength = limit - tailLength - 3;
  return `${value.slice(0, headLength)}...${value.slice(-tailLength)}`;
}
