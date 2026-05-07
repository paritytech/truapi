import { services as generatedServices } from "@parity/truapi/playground/services";
import type {
  MethodInfo,
  ServiceInfo,
} from "@parity/truapi/playground/services";

export type { MethodInfo, ServiceInfo };
export const services: ServiceInfo[] = generatedServices;
