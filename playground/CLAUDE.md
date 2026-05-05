# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Is

An interactive explorer for the TrUAPI — the Host API surface exposed to products running inside the Polkadot Desktop Browser webview. The app must be opened from within a Host environment; the `injectSpektrExtension()` bridge is only available in that context.

To develop locally, run `yarn dev` and open the app via `https://dot.li/localhost:3000` inside the Desktop Host.

## Commands

```bash
yarn dev          # Start Next.js dev server (port 3000)
yarn build        # Build static export to out/
yarn start        # Serve out/ locally
yarn lint         # Run ESLint
yarn lint:fix     # Auto-fix ESLint issues
```

There is no test command — validation is done interactively in the app UI.

## Architecture

**Stack:** Next.js 15 (static export), React 19, TypeScript.  
**Output:** `out/` — deployed to DotNS via GitHub Actions on push to `main`.

### Key Files

| File | Role |
|------|------|
| `src/lib/services.ts` | Single source of truth for all methods: name, type, description, requestDescription, defaultRequest, noParams |
| `src/lib/host-api-bridge.ts` | `methodMap` wires `"ServiceName/method_name"` → `[serviceField, clientMethod, isStream]` on the generated `@truapi/client`; exports `getMethodBinding`, `isMethodSupported`, `stringify` |
| `src/lib/transport.ts` | Singleton `Provider`/`Transport`/`TrUApiClient` over iframe postMessage or webview MessagePort, using `@truapi/client` |
| `src/components/ServiceTable.tsx` | Method browser: renders all services/methods with type badges and description |
| `src/components/MethodView.tsx` | Per-method view: request editor, call/subscribe, response display |
| `src/app/page.tsx` | Root: manages connection status and service/method selection state |

### Adding a Method

1. Add an entry to `services.ts` with `name`, `type`, `description`, and optionally `defaultRequest`, `requestDescription`, or `noParams: true`.
2. Add a corresponding entry to `methodMap` in `host-api-bridge.ts`: `'ServiceName/method_name': ['serviceField', 'clientMethod', isStream]`. `serviceField` is the camelCased trait name on `TrUApiClient` (e.g. `accountManagement`); `clientMethod` is the camelCased method on that service class (e.g. `accountGet`).

Omitting step 2 is valid — the method will appear with a "Not supported" badge until wired up.

### Call Flow

```
ServiceTable click → page.tsx setSelection
  → MethodView: getMethodBinding(service, method)
    → methodMap lookup → wraps client[serviceField][clientMethod] in a call or subscribe binding
      → unary: client[svc][m](req) → { success, value } → { ok, data }
      → stream: client[svc][m](req, onEvent) → Unsubscribe
```

The generated client wraps requests in the V2 versioned envelope internally, so callers pass the inner request value directly. Multi-parameter methods (e.g. `accountCreateProof(productAccountId, ringLocation, context)`) take the request as a JSON array.

### noParams Flag

Methods that take no parameters should set `noParams: true` in `services.ts`. This hides the request textarea in the UI and passes `null` to the binding instead of parsing JSON.

### Transport

`transport.ts` auto-detects environment (iframe vs webview) and exposes singletons `getTransport()` and `getClient()`. The first call to `subscribeConnectionStatus()` triggers a `host_handshake(1)` round-trip. Never create multiple transport or client instances.

In iframe mode the playground talks to its parent window via `postMessage` carrying SCALE-encoded `Uint8Array` frames. In webview mode it pulls a `MessagePort` from `window.__HOST_API_PORT__` (set by the native host) and uses `createMessagePortProvider`.
