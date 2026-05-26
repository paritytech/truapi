export interface MethodInfo {
  name: string;
  type: "unary" | "subscription";
  /** TS-shaped signature for the method (e.g. `getAccount(request: HostAccountGetRequest): Promise<…>`). */
  signature?: string;
  /** Cargo-doc URL fragment for this method (relative to the rustdoc root for the truapi crate). */
  docUrl?: string;
  description?: string;
  requestDescription?: string;
  exampleSource?: string;
}

export interface ServiceInfo {
  name: string;
  methods: MethodInfo[];
}
