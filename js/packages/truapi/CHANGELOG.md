# @parity/truapi

## 0.3.1

### Patch Changes

- Fixed `HostPaymentTopUpError` SCALE variant ordering: `PartialPayment` (index 2) now precedes `Unknown` (index 3), matching the canonical wire layout.

## 0.3.0

### Minor Changes

- **Breaking:** Removed CoinPayment (Coinage) host API (RFC 0017 rolled back). Products using `coinPayment.*` methods must migrate before upgrading.

  **Breaking:** `Theme` enum replaced by `ThemeName` and `ThemeVariant`; `HostThemeSubscribeItem` now carries `name` and `variant` fields instead of `theme` (RFC 0022).

  Added `Coins` variant to `PaymentTopUpSource` for direct coin-key top-ups (RFC 0021). Added `PartialPayment` error variant to `HostPaymentTopUpError`.

## 0.1.0

### Minor Changes

- Initial public release of `@parity/truapi`: TrUAPI transport, SCALE codecs, and the generated TypeScript API client for protocol v1.0.
