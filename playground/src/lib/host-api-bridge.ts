export function stringify(value: unknown): string {
  return JSON.stringify(
    value,
    (_, v) => (typeof v === "bigint" ? v.toString() + "n" : v),
    2,
  );
}
