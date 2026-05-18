export interface MethodInfo {
  name: string;
  type: "unary" | "subscription";
  description?: string;
  requestDescription?: string;
  defaultRequest?: string;
  noParams?: boolean;
  exampleSource?: string;
  exampleFunctionName?: string;
}

export interface ServiceInfo {
  name: string;
  methods: MethodInfo[];
}
