import { services as generatedServices } from "@parity/truapi/playground/services";
import { servicesForExecution } from "@parity/truapi/playground/services-types";
import type {
  MethodInfo,
  ProductExecutionKind,
  ServiceInfo,
} from "@parity/truapi/playground/services-types";

export type { MethodInfo, ProductExecutionKind, ServiceInfo };
export { servicesForExecution };

export const services: ServiceInfo[] = servicesForExecution(
  generatedServices,
  "Spa",
);
