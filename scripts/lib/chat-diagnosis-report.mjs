/**
 * Decode a SCALE-encoded iOS Chat text message stored in CoreData.
 *
 * Mirrors the generated `ChatMessageContent` codec in `@parity/truapi`
 * (Text variant only) so this script stays runnable without building the
 * TS package first.
 */
export function decodeTextMessage(hex) {
  const encoded = Buffer.from(hex, "hex");
  if (encoded[0] !== 0) return undefined;
  const compact = decodeScaleCompact(encoded, 1);
  const start = 1 + compact.bytes;
  return encoded.subarray(start, start + compact.value).toString("utf8");
}

/** Title line the Chat diagnosis worker renders its report under. */
export const CHAT_DIAGNOSIS_HEADING = "## Truapi Chat Diagnosis";

/** Validate a successful Chat report and attach the native host label. */
export function labelChatDiagnosisReport(report, host) {
  const counts = report.match(/\*\*(\d+) success · (\d+) failed\*\*/);
  if (
    !report.startsWith(CHAT_DIAGNOSIS_HEADING) ||
    !counts ||
    counts[1] === "0" ||
    counts[2] !== "0" ||
    report.includes("❌")
  ) {
    throw new Error(`Chat diagnosis reported a failure:\n${report}`);
  }
  return report.replace(
    CHAT_DIAGNOSIS_HEADING,
    `## Truapi ${host} Chat Diagnosis`,
  );
}

function decodeScaleCompact(encoded, offset) {
  const first = encoded[offset];
  const mode = first & 0b11;
  if (mode === 0) return { value: first >> 2, bytes: 1 };
  if (mode === 1) {
    return { value: encoded.readUInt16LE(offset) >> 2, bytes: 2 };
  }
  if (mode === 2) {
    return { value: encoded.readUInt32LE(offset) >>> 2, bytes: 4 };
  }
  throw new Error(
    "Large SCALE compact values are not expected in Chat reports",
  );
}
