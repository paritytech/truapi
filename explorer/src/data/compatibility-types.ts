// Schema for the host × method compatibility matrix consumed by the explorer.
// The values are produced by the diagnosis run inside the playground; the
// committed `compatibility.ts` is rewritten by
// `explorer/scripts/aggregate-diagnosis-matrix.mjs` (`npm run generate-matrix`).

export type CompatStatus = "pass" | "fail";

export interface CompatHost {
  /// Column label — typically the host mode (`Web` / `Desktop` / `Android` /
  /// `iOS`), suffixed with a filename when several reports share the same mode.
  label: string;
  mode: "Web" | "Desktop" | "Android" | "iOS" | "Unknown";
  /// `_Generated:` timestamp copied from the source report, so the page can
  /// surface how fresh each host's measurement is.
  reportedAt: string;
}

export interface CompatMethodRow {
  /// `Service/method` identifier matching the explorer's `MethodInfo.name`
  /// scoped by its parent service.
  id: string;
  /// Keyed by `CompatHost.label`. `null` means the method was absent from that
  /// host's report (e.g. the report predates the method).
  results: Record<string, CompatStatus | null>;
}

export interface CompatibilityMatrix {
  generatedAt: string;
  hosts: CompatHost[];
  methods: CompatMethodRow[];
}
