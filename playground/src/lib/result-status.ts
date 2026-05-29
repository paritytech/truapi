import { stringify } from "./host-api-bridge";
import type { LogEntry } from "./example-runner";

function isNeverthrowErr(value: unknown): value is { error: unknown } {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { isErr?: unknown }).isErr === "function" &&
    (value as { isErr: () => boolean }).isErr()
  );
}

// Detect whether a resolved call value or its captured logs represent an
// errored call. Returns the error text to display, or `null` when the call
// genuinely succeeded.
//
// Generated examples self-handle the Result via
// `result.match(v => console.log(v), e => console.error(e))`, so an Err
// surfaces as an error-level log rather than a thrown exception. Some examples
// instead `return result` / `console.log(result)`, leaving the neverthrow Err
// as the resolved value. Either is treated as an errored call. A bare
// `{ tag, value }` success response is not an error — only an actual neverthrow
// Err or an error-level log counts, so legitimate tagged-union success
// responses stay successful.
export function errorTextFrom(value: unknown, logs: LogEntry[]): string | null {
  if (isNeverthrowErr(value)) {
    return stringify(value.error) ?? String(value.error);
  }
  const errorLogs = logs.filter((l) => l.level === "error");
  if (errorLogs.length > 0) {
    return errorLogs.map((l) => l.text).join("\n");
  }
  return null;
}
