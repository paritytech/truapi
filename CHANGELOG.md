# Changelog

All notable changes to the TrUAPI protocol are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
generated from [Conventional Commits](https://www.conventionalcommits.org/).

## [0.3.0] - 2026-06-02

### RFCs

- **Accepted:** RFC-0020: Remove `context` from `create_transaction` and mirror in Accounts Protocol
- **Accepted:** Add Coins variant to PaymentTopUpSource
- **Accepted:** Extended theme subscribe API
- **Withdrawn:** Host API root account access
- **Withdrawn:** Simple Group Chat

### Added

- extend theme subscribe API with named themes
- proc-macro envelopes + conversion traits
- add Next variant to Version enum
- diagnosis screen + host compatibility matrix (#143)
- show method examples and deep-link to the hosted playground
- require a ```ts example on every trait method
- add version lifecycle tooling and next/ staging module
- host compatibility matrix page
- codegen-driven explorer site with version snapshots (#130)
- implement RFC-0020 Rust types for create_transaction
- add host-side codegen and @parity/truapi-host package (#77)

### Changed

- drop redundant playground link from method example
- Add compatibility parsed reports for web and desktop
- simplify for now
- remove unused Version enum and IntoVersion trait
- update versioned types
- Update RFC index
- Specify deployment environment for Playground (#142)
- Update README references
- Parse diagnosis reports into matrix
- remove JSON-RPC from 0.2.0 snapshot
- Generate diagnosis matrix
- align spec to actual implementation
- Remove RFC 0011-simple-group-chat
- Rfc skill (#131)
- small cleanups extracted from #96 prep work (#124)
- add WELL_KNOWN_CHAINS constants, use in examples (#128)
- Remove RFC 0010-get-root-account
- Notes from 12.05 Working Group Review
- Update 0020-create-transaction.md
- Create 0020-create-transaction.md
- Add Rust trait update checklist item to RFC PR template
- Auto-number RFCs on merge via CI
- complete withChainHeadFollow on Stop instead of erroring
- scope withChainHeadFollow subscriptionId per subscription
- extract withChainHeadFollow, drop `any` from examples, fix GenericError wire
- cargo-doc: serve static.files at site root so rustdoc CSS resolves (#119)
- fall back to ancestorOrigins when referrer is empty (#120)
- sync Cargo.toml version and auto-create GitHub Release (#113)
- render offline UI on GH Pages instead of stuck splash (#118)
- Playground: Monaco editor + rxjs + cargo-doc links + deep links (#116)
- Fix wire ID collision: shift CoinPayment IDs to 136+
- Rename listen_for to listen_for_payment
- RFC 0019: mark as breaking change
- Align RFC 0019 method names and error type with trait renames
- Rename push_notification methods for clarity
- Rename PushNotificationError to HostPushNotificationError
- RFC 0017: remove CoinPaymentInvoice type and align method names
- Remove version field from RFC pseudocode CoinPaymentCheque
- Fix codegen: collect error wrappers for ResultSubscription methods
- Address review comments: remove version field, error aliases, and Resolvable type
- Drop host_coin_payment_ prefix from CoinPayment trait methods
- move notification methods from System to Notifications trait
- implement RFC 0019 scheduled push notifications
- Remove unused PaymentPurse alias and CoinPaymentInvoice type
- Rename HostPaymentRequestRequest to HostPaymentRequest (#83)
- RFC 0017: add CoinPayment host API
- updated tokens with the ones from new design system

### Fixed

- clean up cut-version.sh
- qualify method routes by service
- harden contiguity check and pin multi-version codec indices
- Update Paseo Next V2 Genesis hash
- make examples valid against the generated client
- remove unused HostCreateTransactionWithLegacyAccountRequest
- protocol document
- removed JAM codec mention
- restored indices
- display for RemotePermissionRequest
- fold RFC17 review cleanup
- rename host_push_notification_cancel to push_notification_cancel
- align CoinPayment trait with native async traits

### Removed

- roll back the CoinPayment (Coinage) host API
- remove Version::Next — unused until V2 types exist

## [0.1.0] - 2026-05-15

### RFCs

- **Accepted:** RFC Title
- **Accepted:** Permission Model for Host API
- **Accepted:** Payment Host API
- **Accepted:** RFC-0007: Deterministic Entropy Derivation for Products
- **Accepted:** Statement Store Host API v0.2
- **Accepted:** RFC-0009: Unauthenticated Product Access
- **Accepted:** RFC-0010: W3S Allowance Management in TrUAPI
- **Accepted:** Host API root account access
- **Accepted:** Simple Group Chat
- **Accepted:** RFC-0015: Get User Primary DotNS Name
- **Accepted:** Scheduled Push Notifications

### Added

- reject wire ids that collide with RESERVED_WIRE_IDS

### Changed

- @parity/truapi 0.1.0 — drop --ignore-scripts from install (#91)
- @parity/truapi-0.1.0 (#89)
- @parity/truapi-0.1.0
- Add release template
- Replace release bot with [release: PR title] gate
- Add release workflow to publish @parity/truapi via npm_publish_automation
- ignore generated TS outputs in git (#73)
- Refactor codegen (#68)
- remove public_key from HostGetUserIdResponse
- parse version from type prefix instead of hard-coding V01
- drop "0." from protocol version label
- rewrite to match actual repo structure and RFC CI requirements
- update generated types after macro doc comment changes
- address review: future-proof docs, auto-generate versioned wrapper doc comments
- drop v02 module: merge all types into v01, remove codegen discriminant hack
- tighten SubscriptionError assertions on malformed-receive and provider-close
- collapse observer error to single SubscriptionError type
- fix fmt and regenerate TS client
- Update rust/crates/truapi/src/api/calls.rs
- Update rust/crates/truapi-codegen/src/typescript.rs
- Bump next from 15.5.15 to 15.5.18 in /playground
- rename chainHeadFollow → chainHeadFollowSubscribe
- fix client example return types and regenerate
- regenerate TS client and examples
- fix legacy sign-payload example and fmt
- add V2 HostSignPayloadWithLegacyAccountRequest
- truapi-codegen: emit HexString import in generated client.ts
- add JsonRpc, Theme, ResourceAllocation traits + host_request_login
- add remote_preimage_submit + statement_store_create_proof_authorized
- add host_sign_*_with_legacy_account (wire 34–37)
- rename remote_chain_head_follow → remote_chain_head_follow_subscribe
- fix fmt
- drop host_chat_create_simple_group entirely
- move host_chat_create_simple_group off colliding wire ID 130
- emit plain HexString name + drop dead Uint8Array parsers
- update readme
- update
- align with host-product-sdk via HexString codec, drop dead helpers
- Require truapi interface changes in RFC PRs
- Add RFC validation CI workflow
- Rename deploy-playground CI file
- Fix submodule: recursive for workflows
- PR review
- simplify wire-table
- @parity/truapi: drop unused encodeWireMessage/decodeWireMessage from public surface
- rename /page diagnostics route to /diagnostics
- @parity/truapi: add publish metadata and dispose() handle
- tighten codegen and add v02 RemotePermission::PreimageSubmit
- fix ci
- fixes
- update types
- update
- fixes
- Address PR review findings
- add back doc site
- nit
- fixes
- renaming
- renaming
- rename stuff
- Address PR review findings
- updat rust code
- fix
- Fixes in RFC
- Propagate Rust doc comments to generated TS client
- Tighten review nits: deploy concurrency, BigInt regex, format args
- Pick V1 wrapper in codegen so legacy hosts decode every method
- Auto-respond to host_handshake_request in @truapi/client
- Fix handshake_response payload so the legacy decoder accepts it
- Answer the host's handshake_request to end the retry loop
- Pin handshake to V1 so legacy host-api accepts it
- Make playground reachable when no host is responding
- Add dotli submodule and top-level CLAUDE.md
- Surface subscription id; restore chain-head ephemeral-follow logic
- Add RFC 0012 for scheduled push notifications
- Promote truapi crate, add codegen, drop legacy docsite
- Add dev server proxy for legacy URL redirects
- Prepare for repo rename from truapi-explorer to truapi
- Align v0.2 API definitions with triangle-js-sdks implementation
- Migrate RFC-0014 (Get User Primary DotNS Name) as RFC-0015
- Fix TopicFilter to enum with MatchAll/MatchAny variants
- RFC-0011: Simple Group Chat
- Update RFC index with 0010-allowance entry
- RFC-0010: W3S Allowance Management in TrUAPI
- Migrate feature index and accepted RFCs from triangle-js-sdks
- Migrate PR templates and CONTRIBUTING guide from triangle-js-sdks
- Migrate host-api-protocol design doc from triangle-js-sdks
- Update Contacts API note in v02-changes.md
- Add draggable sidebar resizer with persisted width and double-click reset
- Bump vite to 8.0.8 to patch dev-server path traversal and file-read advisories
- Fix TypeScript narrowing in Fields and Variants map callbacks
- Use CSS grid for Fields and Variants tables to align columns across rows
- Fix long variant/field names overlapping right column on type pages
- Redirect legacy /host-api-explorer URLs to /truapi-explorer
- Promote v0.2 from preview to stable and make it the default version
- Update v02-changes.md with additional document links
- Add README.md
- Add Rust docs and v0.2 change doc
- Add v02 spec
- Change api-spec to v02
- Correct the vite path name
- Rename more thoroughly
- Rename host api to truAPI and add truapi-spec
- Bump brace-expansion
- Bump picomatch from 4.0.3 to 4.0.4
- Bump flatted from 3.4.1 to 3.4.2
- Make mobile friendly
- Clean up types (2)
- Clean up types
- Link types properly (2)
- Link types properly
- Improve readability
- UI improvements
- Replace iframe terminology with sandbox
- Fix GitHub Pages SPA routing
- Remove unused variable in TypesPage
- Add .npmrc to resolve Vite 8 / Tailwind peer dep conflict
- Initial commit: Host API Protocol Explorer

### Fixed

- clippy needless_borrow and stale type import

