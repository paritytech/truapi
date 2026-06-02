# @parity/truapi

## 0.3.0

### Breaking Changes

- Removed CoinPayment (Coinage) host API (RFC 0017 rolled back).
- `Theme` enum replaced by `ThemeName` and `ThemeVariant`; `HostThemeSubscribeItem` now carries `name` and `variant` fields instead of `theme`.

### Minor Changes

- Extended theme subscribe API: products can distinguish named host themes beyond light/dark (RFC 0022).
- Added `Coins` variant to `PaymentTopUpSource` for direct coin-key top-ups (RFC 0021).
- Added `PartialPayment` error variant to `HostPaymentTopUpError`.

## 0.1.0

### Minor Changes

- Initial public release of `@parity/truapi`: TrUAPI transport, SCALE codecs, and the generated TypeScript API client for protocol v1.0.
