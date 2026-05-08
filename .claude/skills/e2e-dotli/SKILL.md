---
name: e2e-dotli
description: Run the playground end-to-end inside the dotli host iframe, either via the automated Playwright suite or by driving it manually in a browser. Use to verify the wire protocol after Rust/codegen/transport changes.
---

# End-to-end inside dotli

Mirrors step 6 of `docs/local-e2e-testing.md`. The static checks do not
exercise the wire protocol; this is what does.

## Automated (Playwright)

Preferred for both local verification and CI. From `playground/`:

```bash
yarn e2e            # headless run; spins up dotli preview + next dev
yarn e2e:ui         # Playwright UI mode for debugging
yarn e2e:headed     # headed run with browser visible
```

The Playwright config (`playground/playwright.config.ts`) starts both
servers via the `webServer` array and the tests live in
`playground/tests/e2e/`:

- `handshake.spec.ts` — the connection chip flips to "Host Linked"
  within ~5s. Proves the `host_handshake_request` →
  `host_handshake_response` round trip.
- `unary.spec.ts` — drives `Account Management → host_account_get`,
  clicks **Call**, asserts a non-empty response. Proves the full
  encode → wire frame → host → wire frame → decode pipeline.
- `subscription.spec.ts` — drives a subscription method, asserts at
  least one stream entry, then clicks **Stop**. Proves the
  `_start` / `_receive` / `_stop` lifecycle.

When a test fails, traces and screenshots land in
`playground/test-results/` — open the `.zip` with
`npx playwright show-trace`.

## Manual (browser)

Use when investigating a wire issue or when the automated suite green-
washes a real problem.

### Start dotli's preview server

```bash
cd hosts/dotli
bun run preview            # → http://localhost:5173
# or, for the TrUAPI debug panel:
bun run preview:debugger   # = VITE_APP_DEBUG=true bun run preview
```

`preview:debugger` is recommended whenever you are investigating a wire
issue — the debug panel logs every host↔product TrUAPI frame.

### Start the playground dev server

```bash
cd playground
yarn dev                   # → http://localhost:3000
```

### Open the playground inside dotli

Navigate (in any browser) to `http://localhost:5173/localhost:3000`.

dotli's host parses `/localhost:<port>` as a proxy directive and iframes
the playground. The playground detects the iframe via `window.parent`
and uses the iframe `postMessage` provider.

### Verification flow inside the playground UI

1. The connection chip flips from **Offline** to **Handshaking** to
   **Host Linked** within ~1s. "Host Linked" proves the handshake.
2. Open **Account Management → host_account_get**, keep the default
   request, click **Call**. A success result with a public key proves
   the full pipeline (SCALE encode → wire frame → dotli decode →
   versioned-wrapper unwrap → host handler → wrap → wire frame →
   SCALE decode → `Result.isOk()`).
3. Open a subscription (e.g. **Account Management →
   host_account_connection_status_subscribe**) and click **Subscribe**.
   Pushed events should appear immediately; **Unsubscribe** must stop
   them. Proves the `_start` / `_receive` / `_stop` lifecycle.
4. For chain methods, open **Chain Interaction →
   remote_chain_head_follow** and subscribe. The bridge auto-detects
   dependent methods (header, body, storage, call, unpin, continue,
   stop_operation) and opens an ephemeral follow when
   `followSubscriptionId` is empty — exercising one is enough.
5. If you changed a versioned wrapper, exercise at least one V1-only
   method (e.g. `host_account_get`) and one V0.2-only method (e.g.
   `host_get_user_id`) to confirm both wire variants still decode.

## Failure modes

- Connection chip stays on **Handshaking** → handshake is failing.
  Check:
  - The dotli console for `Unknown wire tag` /
    `Unknown wire discriminant` — wire-table mismatch between dotli's
    vendored `@parity/truapi` and the just-built one.
  - The playground console for `decodeWireMessage` errors — the
    inbound frame's discriminant is unknown (the playground's
    wire-table is stale; rerun the `refresh-playground-snapshot`
    skill).
- A method call hangs → the host either did not receive the frame
  (check dotli's debug panel) or did not respond. The bridge
  auto-responds to `host_handshake_request` only; everything else is on
  the host implementation.
- Playwright `webServer` times out on `bun run preview` → dotli's
  `turbo run build` is slow on a cold cache. The config gives it 5
  minutes; if that is not enough, prebuild `hosts/dotli` once.
