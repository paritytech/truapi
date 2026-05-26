import type { ServiceInfo } from "../playground/services-types.js";

export interface DataTypeField {
  name: string;
  type: string;
  description?: string;
}

/// A type surfaced by the TrUAPI explorer. One entry per public type in the
/// generated client surface (versioned wrapper enums are not surfaced — the
/// explorer routes around them via `MethodInfo.requestType`/`responseType`).
export interface DataType {
  /// Kebab-case identifier (e.g. `host-account-get-request`). Matches the
  /// `requestType`/`responseType`/`errorType` ids emitted on `MethodInfo`.
  id: string;
  /// Public TS type name (e.g. `HostAccountGetRequest`).
  name: string;
  /// Rustdoc module bucket (`account`, `chat`, …) or `shared` when the type
  /// lives outside `api::<service>`.
  category: string;
  /// TypeScript source for the type, ready to render in a code block.
  definition: string;
  /// Free-form doc comment, with the optional `\`\`\`ts ... \`\`\`` example
  /// block stripped.
  description?: string;
  /// Named fields, populated for `struct`-shaped types.
  fields?: DataTypeField[];
  /// Variants, populated for `enum`-shaped types.
  variants?: DataTypeField[];
}

/// One snapshot of the API surface. `id` is `"main"` for the live codegen
/// output, or a semver string (e.g. `"0.1.0"`) for an archived snapshot.
export interface VersionEntry {
  id: string;
  services: ServiceInfo[];
  types: DataType[];
}
