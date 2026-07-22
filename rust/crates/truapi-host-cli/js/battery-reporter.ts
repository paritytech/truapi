import type { DiagnosisCase, DiagnosisRow } from "./diagnosis.ts";

type WriteLine = (line: string) => void;

export class BatteryReporter {
  readonly #write: WriteLine;
  readonly #color: boolean;
  #service: string | undefined;

  constructor(
    write: WriteLine = (line) => console.log(line),
    color = Boolean(process.stdout.isTTY && !process.env.NO_COLOR),
  ) {
    this.#write = write;
    this.#color = color;
  }

  start(plan: DiagnosisCase[]): void {
    const services = new Set(plan.map((test) => test.serviceName)).size;
    const serviceLabel = services === 1 ? "service" : "services";
    this.#write("");
    this.#write(this.#bold("TrUAPI generated-example battery"));
    this.#write(
      this.#dim(
        `${plan.length} examples · ${services} ${serviceLabel} · generated manifest`,
      ),
    );
  }

  paired(value: unknown): void {
    this.#write(
      `${this.#green("✓")} Pairing ready ${this.#dim(`· ${String(value)}`)}`,
    );
  }

  waitingForHost(milliseconds: number): void {
    this.#write(
      this.#dim(
        `• Waiting ${formatDuration(milliseconds)} for signing-host readiness`,
      ),
    );
  }

  reportSaved(path: string): void {
    this.#write(`${this.#green("✓")} Report saved ${this.#dim(`· ${path}`)}`);
  }

  result(row: DiagnosisRow): void {
    if (row.serviceName !== this.#service) {
      this.#service = row.serviceName;
      this.#write("");
      this.#write(this.#bold(row.serviceName));
    }
    const duration = this.#dim(formatDuration(row.durationMs).padStart(8));
    const method = row.methodName.padEnd(42);
    if (row.status === "pass") {
      this.#write(`  ${this.#green("✓")} ${method} ${duration}`);
      return;
    }
    if (row.status === "skipped") {
      this.#write(`  ${this.#yellow("–")} ${method} ${duration}`);
      this.#write(`      ${this.#dim(cleanDetail(row.output))}`);
      return;
    }
    this.#write(`  ${this.#red("×")} ${method} ${duration}`);
    this.#write(`      ${this.#dim(cleanDetail(row.output))}`);
  }

  finish(rows: DiagnosisRow[], elapsedMs: number): void {
    const passed = rows.filter((row) => row.status === "pass").length;
    const failed = rows.filter((row) => row.status === "fail");
    const skipped = rows.filter((row) => row.status === "skipped").length;

    this.#write("");
    this.#write(this.#bold("Summary"));
    this.#write(
      `${this.#green(`${passed} passed`)} · ${
        failed.length === 0
          ? this.#green("0 failed")
          : this.#red(`${failed.length} failed`)
      } · ${this.#yellow(`${skipped} skipped`)} · ${rows.length} total · ${formatDuration(
        elapsedMs,
      )}`,
    );
    if (failed.length > 0) {
      this.#write("");
      this.#write(this.#red(`Failed examples (${failed.length})`));
      for (const row of failed) this.#write(`  - ${row.id}`);
    }
  }

  #bold(value: string): string {
    return this.#style("1", value);
  }

  #dim(value: string): string {
    return this.#style("2", value);
  }

  #green(value: string): string {
    return this.#style("32", value);
  }

  #yellow(value: string): string {
    return this.#style("33", value);
  }

  #red(value: string): string {
    return this.#style("31", value);
  }

  #style(code: string, value: string): string {
    return this.#color ? `\u001b[${code}m${value}\u001b[0m` : value;
  }
}

export function formatDuration(milliseconds: number): string {
  if (milliseconds < 1_000) return `${milliseconds} ms`;
  if (milliseconds < 60_000) return `${(milliseconds / 1_000).toFixed(1)} s`;
  const minutes = Math.floor(milliseconds / 60_000);
  const seconds = Math.round((milliseconds % 60_000) / 1_000);
  return `${minutes}m ${seconds}s`;
}

export function cleanDetail(output: string): string {
  const collapsed = output
    .replace(/\u001b\[[0-9;]*m/g, "")
    .replace(/\s+/g, " ")
    .trim();
  if (collapsed.length === 0) return "no failure detail";
  if (collapsed.length <= 280) return collapsed;
  const tailLength = Math.ceil(280 * 0.6);
  const headLength = 280 - tailLength - 3;
  return `${collapsed.slice(0, headLength)}...${collapsed.slice(-tailLength)}`;
}
