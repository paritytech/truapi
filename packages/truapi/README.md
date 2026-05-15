# @useragent-kit/truapi

Versioned TrUAPI protocol artifacts for products and hosts.

## Usage

```ts
import { manifest, methods, type TrUApiMethodName } from "@useragent-kit/truapi/v0.2";

const firstMethod: TrUApiMethodName = methods[0].name;
console.log(manifest.protocol.version, firstMethod);
```

The raw manifest is also available at `@useragent-kit/truapi/v0.2/manifest.json`.
