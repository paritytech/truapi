/** Coerce an unknown thrown value into a human-readable message string. */
export function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return JSON.stringify(err) ?? String(err);
}
