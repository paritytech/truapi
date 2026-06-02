/** Deployed playground served inside the Polkadot Desktop Host. */
export const HOSTED_PLAYGROUND_URL = "https://truapi-playground.dot.li";

/** Deep link that opens a method in the host-backed playground. */
export function playgroundMethodUrl(service: string, method: string): string {
  const params = new URLSearchParams({ service, method });
  return `${HOSTED_PLAYGROUND_URL}/?${params.toString()}`;
}

/** Deep link that opens the playground's Diagnosis screen. */
export function playgroundDiagnosisUrl(): string {
  return `${HOSTED_PLAYGROUND_URL}/?view=diagnosis`;
}
