/** Communication pattern of a method. */
export type MethodKind = "unary" | "subscription";

/** A single method exposed by a service. */
export interface MethodInfo {
  name: string;
  type: MethodKind;
  signature?: string;
  docUrl?: string;
  description?: string;
  requestDescription?: string;
  exampleSource?: string;
  requestType?: string;
  responseType?: string;
  errorType?: string;
}

/** A grouping of related methods. */
export interface ServiceInfo {
  name: string;
  methods: MethodInfo[];
}

/** A field on a struct or a variant on an enum. */
export interface DataTypeField {
  name: string;
  type: string;
  description?: string;
}

/** A type definition shared between methods. */
export interface DataType {
  id: string;
  name: string;
  category: string;
  definition: string;
  description?: string;
  fields?: DataTypeField[];
  variants?: DataTypeField[];
}

/** One protocol version with all of its services and types. */
export interface VersionEntry {
  id: string;
  services: ServiceInfo[];
  types: DataType[];
}
