export interface MethodInfo {
  name: string;
  type: "unary" | "subscription";
  description?: string;
  requestDescription?: string;
  exampleSource?: string;
  exampleFunctionName?: string;
}

export interface ServiceInfo {
  name: string;
  methods: MethodInfo[];
}
