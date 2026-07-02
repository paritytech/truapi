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
  /** DataType id (kebab-case) of the method's request payload, when applicable. */
  requestType?: string;
  /** DataType id of the method's response. */
  responseType?: string;
  /** DataType id of the method's error. */
  errorType?: string;
  /** Permission requirements for the method. Absent means no permissions needed. */
  permissions?: MethodPermissions;
}

export interface MethodPermissions {
  auth?: string;
  prompt?: string;
  permissionType?: string;
  denialError?: string;
}

export interface ServiceInfo {
  name: string;
  methods: MethodInfo[];
}
