export interface MethodInfo {
  name: string;
  type: "unary" | "subscription";
  /** TS-shaped signature for the method (e.g. `getAccount(request: HostAccountGetRequest): Promise<…>`). */
  signature?: string;
  description?: string;
  requestDescription?: string;
  exampleSource?: string;
}

export interface ServiceInfo {
  name: string;
  methods: MethodInfo[];
}
