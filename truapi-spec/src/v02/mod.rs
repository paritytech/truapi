//! TrUAPI Protocol v0.2 -- trait and type definitions.
//!
//! This module defines the [`TrUApi`] trait containing all TrUAPI v0.2 methods, along with
//! every data type used in their signatures. The three communication patterns are:
//!
//! - **Request-response**: product calls host, host returns a result.
//! - **Subscription**: product subscribes, host pushes values via callback.
//! - **Reverse-subscription**: host initiates, product responds (only used for custom chat
//!   message rendering).

use crate::Subscription;

mod account;
mod chain_interaction;
mod chat;
mod common;
mod custom_renderer;
mod entropy;
mod payment;
mod preimage;
mod signing;
mod statement_store;
mod storage;
mod transaction;

pub use account::*;
pub use chain_interaction::*;
pub use chat::*;
pub use common::*;
pub use custom_renderer::*;
pub use entropy::*;
pub use payment::*;
pub use preimage::*;
pub use signing::*;
pub use statement_store::*;
pub use storage::*;
pub use transaction::*;

// ─── TrUAPI traits ─────────────────────────────────────────────────────────

/// General-purpose TrUAPI methods for feature detection, navigation, and notifications.
pub trait TrUApiCalls {
    /// Queries whether the host supports a specific feature. Currently only the
    /// `Chain` variant exists, carrying a genesis hash to check whether a
    /// specific blockchain is available.
    ///
    /// # Product Function
    ///
    /// `truApi.featureSupported(feature)`
    ///
    /// # Host Handler
    ///
    /// `container.handleFeatureSupported(handler)`
    ///
    /// # Request Description
    ///
    /// Feature enum — Chain(GenesisHash)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Check if Polkadot is supported
    /// const polkadotGenesis = "0x91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3";
    /// const result = await truApi.featureSupported({
    ///   Chain: polkadotGenesis
    /// });
    ///
    /// if (result.isOk) {
    ///   console.log("Polkadot supported:", result.value);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleFeatureSupported((feature, { ok, err }) => {
    ///   if (feature.tag === "Chain") {
    ///     const supported = supportedChains.has(feature.value);
    ///     return ok(supported);
    ///   }
    ///   return ok(false);
    /// });
    /// ```
    fn host_feature_supported(&self, feature: Feature) -> Result<bool, GenericError>;

    /// Requests the host to open a URL, typically in a new browser tab.
    ///
    /// # Product Function
    ///
    /// `truApi.navigateTo(url)`
    ///
    /// # Host Handler
    ///
    /// `container.handleNavigateTo(handler)`
    ///
    /// # Request Description
    ///
    /// The URL to open
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Open an external link
    /// const result = await truApi.navigateTo(
    ///   "https://polkadot.network"
    /// );
    ///
    /// if (result.isErr) {
    ///   console.error("Navigation failed:", result.error);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleNavigateTo((url, { ok, err }) => {
    ///   try {
    ///     window.open(url, "_blank");
    ///     return ok(undefined);
    ///   } catch (e) {
    ///     return err({ PermissionDenied: undefined });
    ///   }
    /// });
    /// ```
    fn host_navigate_to(&self, url: String) -> Result<(), NavigateToError>;

    /// Sends a push notification to the user via the host.
    ///
    /// # Product Function
    ///
    /// `truApi.pushNotification(notification)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePushNotification(handler)`
    ///
    /// # Request Description
    ///
    /// See PushNotification type for fields
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Send a notification with a deeplink
    /// const result = await truApi.pushNotification({
    ///   text: "Your transaction was confirmed!",
    ///   deeplink: "myapp://tx/0xabc123"
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePushNotification((notification, { ok, err }) => {
    ///   showSystemNotification(notification.text, {
    ///     onclick: () => {
    ///       if (notification.deeplink) {
    ///         navigate(notification.deeplink);
    ///       }
    ///     }
    ///   });
    ///   return ok(undefined);
    /// });
    /// ```
    fn host_push_notification(&self, notification: PushNotification) -> Result<(), GenericError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Device and remote permission requests for camera, microphone, HTTP, and transaction access.
pub trait Permissions {
    /// Requests access to a device capability.
    ///
    /// V0.2: extended set of capabilities per
    /// [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66).
    ///
    /// # Product Function
    ///
    /// `truApi.devicePermission(permission)`
    ///
    /// # Host Handler
    ///
    /// `container.handleDevicePermission(handler)`
    ///
    /// # Request Description
    ///
    /// DevicePermission enum: Notifications | Camera | Microphone | Bluetooth | Nfc | Location | Clipboard | OpenUrl | Biometrics
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Request camera access
    /// const granted = await truApi.devicePermission("Camera");
    ///
    /// if (granted.isOk && granted.value) {
    ///   startCamera();
    /// }
    ///
    /// // Request push notification permission
    /// const notifGranted = await truApi.devicePermission("Notifications");
    ///
    /// // Request biometric authentication
    /// const bioGranted = await truApi.devicePermission("Biometrics");
    /// if (bioGranted.isOk && bioGranted.value) {
    ///   enableBiometricLogin();
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleDevicePermission((permission, { ok, err }) => {
    ///   // Show permission dialog to user
    ///   // permission is one of: "Notifications", "Camera", "Microphone",
    ///   //   "Bluetooth", "Nfc", "Location", "Clipboard", "OpenUrl", "Biometrics"
    ///   const granted = await showPermissionDialog(permission);
    ///   return ok(granted);
    /// });
    /// ```
    fn host_device_permission(&self, permission: DevicePermission) -> Result<bool, GenericError>;

    /// Requests permission for one or more remote operations. Batching multiple
    /// entries into a single call lets the host present a single prompt.
    ///
    /// Returns `true` if **all** requested permissions were granted, `false` if
    /// the user denied at least one. Products that need per-entry feedback
    /// should issue individual calls.
    ///
    /// V0.2: accepts `Vec<RemotePermission>` per
    /// [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66).
    ///
    /// # Product Function
    ///
    /// `truApi.permission(requests)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePermission(handler)`
    ///
    /// # Request Description
    ///
    /// Array of RemotePermission entries: Remote(Vector(str)) | WebRtc | ChainSubmit | StatementSubmit
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Request multiple permissions in a single prompt
    /// const allowed = await truApi.permission([
    ///   { Remote: ["api.coingecko.com", "*.example.com"] },
    ///   { ChainSubmit: undefined },
    ///   { StatementSubmit: undefined },
    /// ]);
    ///
    /// if (allowed.isOk && allowed.value) {
    ///   // All permissions granted, proceed
    ///   const price = await fetch("https://api.coingecko.com/...");
    /// }
    ///
    /// // Request WebRTC permission
    /// const webrtcAllowed = await truApi.permission([
    ///   { WebRtc: undefined },
    /// ]);
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePermission((permissions, { ok, err }) => {
    ///   // permissions is an array of RemotePermission entries
    ///   // Present a single prompt to the user for all requested permissions
    ///   for (const perm of permissions) {
    ///     if (perm.tag === "Remote") {
    ///       if (!checkDomainAllowlist(perm.value)) {
    ///         return ok(false);
    ///       }
    ///     }
    ///     if (perm.tag === "ChainSubmit") {
    ///       if (!userHasApprovedTxSubmission) {
    ///         return ok(false);
    ///       }
    ///     }
    ///   }
    ///   return ok(true);
    /// });
    /// ```
    fn remote_permission(&self, permissions: Vec<RemotePermission>) -> Result<bool, GenericError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Scoped key-value storage. The host namespaces keys so different products cannot read each other's data.
pub trait LocalStorage {
    /// Reads a value from the scoped key-value store.
    ///
    /// # Product Function
    ///
    /// `truApi.localStorageRead(key)`
    ///
    /// # Host Handler
    ///
    /// `container.handleLocalStorageRead(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Read a stored preference
    /// const result = await truApi.localStorageRead("user-theme");
    ///
    /// if (result.isOk && result.value !== null) {
    ///   const theme = new TextDecoder().decode(result.value);
    ///   applyTheme(theme);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleLocalStorageRead((key, { ok, err }) => {
    ///   const namespacedKey = `${productId}:${key}`;
    ///   const value = localStorage.getItem(namespacedKey);
    ///   return ok(value ? new TextEncoder().encode(value) : null);
    /// });
    /// ```
    fn host_local_storage_read(
        &self,
        key: StorageKey,
    ) -> Result<Option<StorageValue>, StorageError>;

    /// Writes a value to the scoped key-value store.
    ///
    /// # Product Function
    ///
    /// `truApi.localStorageWrite([key, value])`
    ///
    /// # Host Handler
    ///
    /// `container.handleLocalStorageWrite(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Store a user preference
    /// const theme = new TextEncoder().encode("dark");
    /// const result = await truApi.localStorageWrite([
    ///   "user-theme",
    ///   theme
    /// ]);
    ///
    /// if (result.isErr) {
    ///   console.error("Storage write failed:", result.error);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleLocalStorageWrite(([key, value], { ok, err }) => {
    ///   const namespacedKey = `${productId}:${key}`;
    ///   try {
    ///     localStorage.setItem(namespacedKey, new TextDecoder().decode(value));
    ///     return ok(undefined);
    ///   } catch (e) {
    ///     return err({ Full: undefined });
    ///   }
    /// });
    /// ```
    fn host_local_storage_write(
        &self,
        key: StorageKey,
        value: StorageValue,
    ) -> Result<(), StorageError>;

    /// Clears a value from the scoped key-value store.
    ///
    /// # Product Function
    ///
    /// `truApi.localStorageClear(key)`
    ///
    /// # Host Handler
    ///
    /// `container.handleLocalStorageClear(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Clear stored data
    /// const result = await truApi.localStorageClear("user-theme");
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleLocalStorageClear((key, { ok, err }) => {
    ///   const namespacedKey = `${productId}:${key}`;
    ///   localStorage.removeItem(namespacedKey);
    ///   return ok(undefined);
    /// });
    /// ```
    fn host_local_storage_clear(&self, key: StorageKey) -> Result<(), StorageError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Product-specific account derivation, alias retrieval, ring VRF proofs, connection status, and user identity.
pub trait AccountManagement {
    /// Retrieves a product-specific derived account. The product provides a
    /// product identifier and derivation index; the host derives a unique public
    /// key for that combination.
    ///
    /// # Product Function
    ///
    /// `truApi.accountGet(productAccountId)`
    ///
    /// # Host Handler
    ///
    /// `container.handleAccountGet(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId is a Tuple(DotNsIdentifier, DerivationIndex)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Get the product account for "my-product" with index 0
    /// const result = await truApi.accountGet([
    ///   "my-product.dot",  // DotNS identifier
    ///   0               // derivation index
    /// ]);
    ///
    /// if (result.isOk) {
    ///   const { publicKey, name } = result.value;
    ///   console.log("Account:", name ?? "unnamed");
    ///   console.log("Key:", toHex(publicKey));
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleAccountGet(([dotNsId, derivationIndex], { ok, err }) => {
    ///   if (!currentUser) {
    ///     return err({ NotConnected: undefined });
    ///   }
    ///   const account = deriveProductAccount(
    ///     currentUser, dotNsId, derivationIndex
    ///   );
    ///   return ok({
    ///     publicKey: account.publicKey,
    ///     name: account.displayName ?? null,
    ///   });
    /// });
    /// ```
    fn host_account_get(
        &self,
        product_account_id: ProductAccountId,
    ) -> Result<Account, RequestCredentialsError>;

    /// Retrieves a contextual alias (ring VRF based) for a product account.
    ///
    /// # Product Function
    ///
    /// `truApi.accountGetAlias(productAccountId)`
    ///
    /// # Host Handler
    ///
    /// `container.handleAccountGetAlias(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId is a Tuple(DotNsIdentifier, DerivationIndex)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Get a contextual alias for privacy-preserving identity
    /// const result = await truApi.accountGetAlias([
    ///   "my-product.dot",
    ///   0
    /// ]);
    ///
    /// if (result.isOk) {
    ///   const { context, alias } = result.value;
    ///   // Use alias for anonymous interactions
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleAccountGetAlias(([dotNsId, derivationIndex], { ok, err }) => {
    ///   if (!currentUser) {
    ///     return err({ NotConnected: undefined });
    ///   }
    ///   const alias = computeContextualAlias(
    ///     currentUser, dotNsId, derivationIndex
    ///   );
    ///   return ok(alias);
    /// });
    /// ```
    fn host_account_get_alias(
        &self,
        product_account_id: ProductAccountId,
    ) -> Result<ContextualAlias, RequestCredentialsError>;

    /// Creates a ring VRF proof for a product account against a specific ring.
    ///
    /// # Product Function
    ///
    /// `truApi.accountCreateProof(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleAccountCreateProof(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId, RingLocation, and context bytes
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a ring VRF proof
    /// const result = await truApi.accountCreateProof([
    ///   ["my-product.dot", 0],          // ProductAccountId
    ///   {                              // RingLocation
    ///     genesisHash: polkadotGenesis,
    ///     ringRootHash: "0xabcdef...",
    ///     hints: { palletInstance: 42 },
    ///   },
    ///   contextBytes                   // Bytes - context data
    /// ]);
    ///
    /// if (result.isOk) {
    ///   const proof = result.value; // RingVrfProof
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleAccountCreateProof(
    ///   ([productAccountId, ringLocation, context], { ok, err }) => {
    ///     const proof = ringVrf.createProof(
    ///       productAccountId, ringLocation, context
    ///     );
    ///     if (!proof) {
    ///       return err({ RingNotFound: undefined });
    ///     }
    ///     return ok(proof);
    ///   }
    /// );
    /// ```
    fn host_account_create_proof(
        &self,
        product_account_id: ProductAccountId,
        ring_location: RingLocation,
        context: Bytes,
    ) -> Result<RingVrfProof, CreateProofError>;

    /// Retrieves the user's non-product accounts (e.g., their main wallet
    /// account, not derived per-product).
    ///
    /// # Product Function
    ///
    /// `truApi.getNonProductAccounts()`
    ///
    /// # Host Handler
    ///
    /// `container.handleGetNonProductAccounts(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Get the user's wallet accounts
    /// const result = await truApi.getNonProductAccounts();
    ///
    /// if (result.isOk) {
    ///   for (const account of result.value) {
    ///     console.log(account.name, toHex(account.publicKey));
    ///   }
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleGetNonProductAccounts((_, { ok, err }) => {
    ///   if (!currentUser) {
    ///     return err({ NotConnected: undefined });
    ///   }
    ///   return ok(currentUser.walletAccounts.map(a => ({
    ///     publicKey: a.publicKey,
    ///     name: a.displayName ?? null,
    ///   })));
    /// });
    /// ```
    fn host_get_non_product_accounts(&self) -> Result<Vec<Account>, RequestCredentialsError>;

    /// Subscribes to changes in the user's authentication state. The host pushes
    /// `Connected` or `Disconnected` whenever the auth state changes.
    ///
    /// # Product Function
    ///
    /// `truApi.accountConnectionStatusSubscribe(void, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleAccountConnectionStatusSubscribe(handler)`
    ///
    /// # Response Description
    ///
    /// Status enum: "disconnected" | "connected"
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Watch for authentication changes
    /// const sub = truApi.accountConnectionStatusSubscribe(
    ///   undefined,
    ///   (status) => {
    ///     if (status === "connected") {
    ///       showWalletUI();
    ///     } else {
    ///       showConnectButton();
    ///     }
    ///   }
    /// );
    ///
    /// // Later: clean up
    /// sub.unsubscribe();
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleAccountConnectionStatusSubscribe(
    ///   (params, send, interrupt) => {
    ///     // Send initial status
    ///     send(currentUser ? "connected" : "disconnected");
    ///
    ///     // Watch for changes
    ///     const unsub = authStore.onChange((user) => {
    ///       send(user ? "connected" : "disconnected");
    ///     });
    ///
    ///     return () => unsub(); // cleanup
    ///   }
    /// );
    /// ```
    fn host_account_connection_status_subscribe(&self) -> Subscription<AccountConnectionStatus>;

    /// Returns the user's primary DotNS account identifier. Requires JIT
    /// user approval on first call.
    ///
    /// V0.2.
    ///
    /// # Product Function
    ///
    /// `truApi.getUserId()`
    ///
    /// # Host Handler
    ///
    /// `container.handleGetUserId(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Get the user's primary identity
    /// const result = await truApi.getUserId();
    ///
    /// if (result.isOk) {
    ///   const { dotNsIdentifier, publicKey } = result.value;
    ///   console.log("User:", dotNsIdentifier);
    ///   console.log("Public key:", toHex(publicKey));
    /// } else if (result.error.tag === "Rejected") {
    ///   console.log("User declined identity disclosure");
    /// } else if (result.error.tag === "NotConnected") {
    ///   console.log("User is not logged in");
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleGetUserId((_, { ok, err }) => {
    ///   if (!currentUser) {
    ///     return err({ NotConnected: undefined });
    ///   }
    ///   // Prompt user for permission to disclose identity
    ///   const approved = await showIdentityDisclosureDialog();
    ///   if (!approved) {
    ///     return err({ Rejected: undefined });
    ///   }
    ///   return ok({
    ///     dotNsIdentifier: currentUser.dotNsId,
    ///     publicKey: currentUser.primaryPublicKey,
    ///   });
    /// });
    /// ```
    fn host_get_user_id(&self) -> Result<UserIdentity, UserIdentityError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Transaction payload signing, raw message signing, and full transaction creation.
pub trait Signing {
    /// Requests the host to sign a Substrate transaction payload. The host
    /// typically shows a confirmation modal to the user.
    ///
    /// # Product Function
    ///
    /// `truApi.signPayload(payload)`
    ///
    /// # Host Handler
    ///
    /// `container.handleSignPayload(handler)`
    ///
    /// # Request Description
    ///
    /// See SigningPayload type for all fields
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Sign a Substrate extrinsic payload
    /// const result = await truApi.signPayload({
    ///   account: ["my-product.dot", 0],  // ProductAccountId
    ///   blockHash: "0xabc...",
    ///   blockNumber: "0x01",
    ///   era: "0x6502",
    ///   genesisHash: polkadotGenesis,
    ///   method: "0x0500...",    // encoded call data
    ///   nonce: "0x00",
    ///   specVersion: "0x01",
    ///   tip: "0x00",
    ///   transactionVersion: "0x01",
    ///   signedExtensions: ["CheckSpecVersion", "CheckTxVersion"],
    ///   version: 4,
    ///   withSignedTransaction: true,
    /// });
    ///
    /// if (result.isOk) {
    ///   const { signature, signedTransaction } = result.value;
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleSignPayload((payload, { ok, err }) => {
    ///   // payload.account is a ProductAccountId tuple
    ///   const [dotNsId, derivationIndex] = payload.account;
    ///   const userApproved = await showSigningDialog(payload);
    ///   if (!userApproved) {
    ///     return err({ Rejected: undefined });
    ///   }
    ///   const key = deriveProductKey(currentUser, dotNsId, derivationIndex);
    ///   const signature = await key.sign(payload);
    ///   return ok({
    ///     signature,
    ///     signedTransaction: null,
    ///   });
    /// });
    /// ```
    fn host_sign_payload(&self, payload: SigningPayload) -> Result<SigningResult, SigningError>;

    /// Requests the host to sign a raw message (not a transaction).
    ///
    /// # Product Function
    ///
    /// `truApi.signRaw(payload)`
    ///
    /// # Host Handler
    ///
    /// `container.handleSignRaw(handler)`
    ///
    /// # Request Description
    ///
    /// See SigningRawPayload type for fields
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Sign a raw message
    /// const result = await truApi.signRaw({
    ///   account: ["my-product.dot", 0],  // ProductAccountId
    ///   data: { Payload: "Please sign this message to verify ownership" }
    /// });
    ///
    /// // Or sign raw bytes
    /// const result2 = await truApi.signRaw({
    ///   account: ["my-product.dot", 0],
    ///   data: { Bytes: new Uint8Array([1, 2, 3]) }
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleSignRaw((payload, { ok, err }) => {
    ///   const [dotNsId, derivationIndex] = payload.account;
    ///   const userApproved = await showRawSigningDialog(payload);
    ///   if (!userApproved) {
    ///     return err({ Rejected: undefined });
    ///   }
    ///   const key = deriveProductKey(currentUser, dotNsId, derivationIndex);
    ///   const signature = await key.signRaw(payload.data);
    ///   return ok({ signature, signedTransaction: null });
    /// });
    /// ```
    fn host_sign_raw(&self, payload: SigningRawPayload) -> Result<SigningResult, SigningError>;

    /// Requests the host to create and sign a full transaction from a structured
    /// payload, using a product-derived account.
    ///
    /// # Product Function
    ///
    /// `truApi.createTransaction(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleCreateTransaction(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId and a VersionedTxPayload
    ///
    /// # Response Description
    ///
    /// The signed transaction bytes
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a signed transaction using product account
    /// const result = await truApi.createTransaction([
    ///   ["my-product.dot", 0],  // ProductAccountId
    ///   {
    ///     v1: {
    ///       signer: null,        // host picks the signer
    ///       callData: "0x0500...", // SCALE-encoded Call
    ///       extensions: [
    ///         { id: "CheckSpecVersion", extra: "0x", additionalSigned: "0x01000000" },
    ///       ],
    ///       txExtVersion: 0,
    ///       context: {
    ///         metadata: "0x...",
    ///         tokenSymbol: "DOT",
    ///         tokenDecimals: 10,
    ///         bestBlockHeight: 12345678,
    ///       },
    ///     }
    ///   }
    /// ]);
    ///
    /// if (result.isOk) {
    ///   // Submit the signed transaction
    ///   const signedTx = result.value;
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleCreateTransaction(
    ///   ([productAccountId, versionedPayload], { ok, err }) => {
    ///     if (versionedPayload.tag !== "v1") {
    ///       return err({ NotSupported: "Only v1 supported" });
    ///     }
    ///     const tx = buildAndSignTransaction(
    ///       productAccountId, versionedPayload.value
    ///     );
    ///     return ok(tx);
    ///   }
    /// );
    /// ```
    fn host_create_transaction(
        &self,
        product_account_id: ProductAccountId,
        payload: VersionedTxPayload,
    ) -> Result<Bytes, CreateTransactionError>;

    /// Same as [`host_create_transaction`](Signing::host_create_transaction) but uses the
    /// user's main account instead of a product-derived account.
    ///
    /// # Product Function
    ///
    /// `truApi.createTransactionWithNonProductAccount(payload)`
    ///
    /// # Host Handler
    ///
    /// `container.handleCreateTransactionWithNonProductAccount(handler)`
    ///
    /// # Request Description
    ///
    /// Same VersionedTxPayload structure, without ProductAccountId
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create transaction with user's main wallet account
    /// const result = await truApi.createTransactionWithNonProductAccount({
    ///   v1: {
    ///     signer: "5GrwvaEF5...",
    ///     callData: "0x0500...",
    ///     extensions: [],
    ///     txExtVersion: 0,
    ///     context: {
    ///       metadata: "0x...",
    ///       tokenSymbol: "DOT",
    ///       tokenDecimals: 10,
    ///       bestBlockHeight: 12345678,
    ///     },
    ///   }
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleCreateTransactionWithNonProductAccount(
    ///   (versionedPayload, { ok, err }) => {
    ///     const tx = buildAndSignTransaction(
    ///       currentUser.mainAccount, versionedPayload.value
    ///     );
    ///     return ok(tx);
    ///   }
    /// );
    /// ```
    fn host_create_transaction_with_non_product_account(
        &self,
        payload: VersionedTxPayload,
    ) -> Result<Bytes, CreateTransactionError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Chat room management, bot registration, message posting, simple group chats, and custom message rendering.
pub trait Chat {
    /// Registers a chat room with the host.
    ///
    /// # Product Function
    ///
    /// `truApi.chatCreateRoom(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatCreateRoom(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a chat room
    /// const result = await truApi.chatCreateRoom({
    ///   roomId: "general-chat",
    ///   name: "General Discussion",
    ///   icon: "https://example.com/chat-icon.png"
    /// });
    ///
    /// if (result.isOk) {
    ///   console.log("Room status:", result.value.status);
    ///   // "New" or "Exists"
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatCreateRoom((request, { ok, err }) => {
    ///   const existing = chatRooms.get(request.roomId);
    ///   if (existing) {
    ///     return ok({ status: "Exists" });
    ///   }
    ///   chatRooms.set(request.roomId, {
    ///     name: request.name,
    ///     icon: request.icon,
    ///   });
    ///   return ok({ status: "New" });
    /// });
    /// ```
    fn host_chat_create_room(
        &self,
        request: ChatRoomRequest,
    ) -> Result<ChatRoomRegistrationResult, ChatRoomRegistrationError>;

    /// Creates a simple group chat room. Participants join via the returned
    /// deep link. The host handles the group chat UI with default rendering
    /// (no custom elements).
    ///
    /// V0.2: lightweight alternative to the full Chat Extension v2 (deferred
    /// to v0.3).
    ///
    /// # Product Function
    ///
    /// `truApi.chatCreateSimpleGroup(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatCreateSimpleGroup(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a simple group chat
    /// const result = await truApi.chatCreateSimpleGroup({
    ///   roomId: "team-alpha",
    ///   name: "Team Alpha Chat",
    ///   icon: "https://example.com/team-icon.png"
    /// });
    ///
    /// if (result.isOk) {
    ///   const { status, joinLink } = result.value;
    ///   console.log("Room status:", status); // "New" or "Exists"
    ///   console.log("Share this link:", joinLink);
    ///   // Send joinLink to participants so they can join
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatCreateSimpleGroup((request, { ok, err }) => {
    ///   const existing = simpleGroupRooms.get(request.roomId);
    ///   if (existing) {
    ///     return ok({
    ///       status: "Exists",
    ///       joinLink: existing.joinLink,
    ///     });
    ///   }
    ///   const joinLink = generateDeepLink(request.roomId);
    ///   simpleGroupRooms.set(request.roomId, {
    ///     name: request.name,
    ///     icon: request.icon,
    ///     joinLink,
    ///   });
    ///   return ok({ status: "New", joinLink });
    /// });
    /// ```
    fn host_chat_create_simple_group(
        &self,
        request: SimpleGroupChatRequest,
    ) -> Result<SimpleGroupChatResult, ChatRoomRegistrationError>;

    /// Registers a bot identity for chat.
    ///
    /// # Product Function
    ///
    /// `truApi.chatRegisterBot(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatBotRegistration(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Register a bot for automated messages
    /// const result = await truApi.chatRegisterBot({
    ///   botId: "price-bot",
    ///   name: "Price Alert Bot",
    ///   icon: "https://example.com/bot-icon.png"
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatBotRegistration((request, { ok, err }) => {
    ///   const existing = chatBots.get(request.botId);
    ///   if (existing) {
    ///     return ok({ status: "Exists" });
    ///   }
    ///   chatBots.set(request.botId, {
    ///     name: request.name,
    ///     icon: request.icon,
    ///   });
    ///   return ok({ status: "New" });
    /// });
    /// ```
    fn host_chat_register_bot(
        &self,
        request: ChatBotRequest,
    ) -> Result<ChatBotRegistrationResult, ChatBotRegistrationError>;

    /// Posts a message to a chat room. Supports text, rich text, actions, files,
    /// reactions, and custom messages.
    ///
    /// # Product Function
    ///
    /// `truApi.chatPostMessage(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatPostMessage(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Post a text message
    /// const result = await truApi.chatPostMessage({
    ///   roomId: "general-chat",
    ///   payload: { Text: "Hello everyone!" }
    /// });
    ///
    /// // Post an action menu
    /// const result2 = await truApi.chatPostMessage({
    ///   roomId: "general-chat",
    ///   payload: {
    ///     Actions: {
    ///       text: "Choose an option:",
    ///       actions: [
    ///         { actionId: "vote-yes", title: "Vote Yes" },
    ///         { actionId: "vote-no", title: "Vote No" },
    ///       ],
    ///       layout: "Grid"
    ///     }
    ///   }
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatPostMessage(({ roomId, payload }, { ok, err }) => {
    ///   const messageId = generateId();
    ///   chatRooms.get(roomId)?.messages.push({
    ///     id: messageId,
    ///     content: payload,
    ///     timestamp: Date.now(),
    ///   });
    ///   return ok({ messageId });
    /// });
    /// ```
    fn host_chat_post_message(
        &self,
        request: ChatPostMessageRequest,
    ) -> Result<ChatPostMessageResult, ChatMessagePostingError>;

    /// Subscribes to the list of chat rooms the product participates in. The
    /// host pushes the full room list whenever it changes.
    ///
    /// # Product Function
    ///
    /// `truApi.chatListSubscribe(void, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatListSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Watch the room list
    /// const sub = truApi.chatListSubscribe(
    ///   undefined,
    ///   (rooms) => {
    ///     console.log("Current rooms:", rooms);
    ///     rooms.forEach(room => {
    ///       console.log(`  ${room.roomId} as ${room.participatingAs}`);
    ///     });
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatListSubscribe((_, send, interrupt) => {
    ///   // Send initial room list
    ///   send(getRoomsForProduct(productId));
    ///
    ///   const unsub = roomStore.onChange(() => {
    ///     send(getRoomsForProduct(productId));
    ///   });
    ///
    ///   return () => unsub();
    /// });
    /// ```
    fn host_chat_list_subscribe(&self) -> Subscription<Vec<ChatRoom>>;

    /// Subscribes to chat actions (messages posted by peers, button clicks,
    /// commands).
    ///
    /// # Product Function
    ///
    /// `truApi.chatActionSubscribe(void, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatActionSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Listen for chat events
    /// const sub = truApi.chatActionSubscribe(
    ///   undefined,
    ///   (action) => {
    ///     const { roomId, peer, payload } = action;
    ///
    ///     if (payload.tag === "MessagePosted") {
    ///       handleNewMessage(roomId, peer, payload.value);
    ///     } else if (payload.tag === "ActionTriggered") {
    ///       handleAction(payload.value.actionId);
    ///     } else if (payload.tag === "Command") {
    ///       handleCommand(payload.value.command, payload.value.payload);
    ///     }
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatActionSubscribe((_, send, interrupt) => {
    ///   const unsub = chatEvents.on("action", (action) => {
    ///     send(action);
    ///   });
    ///   return () => unsub();
    /// });
    /// ```
    fn host_chat_action_subscribe(&self) -> Subscription<ReceivedChatAction>;

    /// Registers a renderer for custom chat messages (reverse-subscription).
    ///
    /// This is the only method where roles are reversed: the host initiates by
    /// sending a [`CustomMessageRenderRequest`], and the product responds with a
    /// [`CustomRendererNode`] tree via the returned callback.
    ///
    /// # Pattern
    /// reverse-subscription
    ///
    /// # Product Function
    ///
    /// `createProductChatManager().onCustomMessageRenderingRequest(renderer)`
    ///
    /// # Host Handler
    ///
    /// `container.renderChatCustomMessage(msg, callback)`
    ///
    /// # Request Description
    ///
    /// Host sends message details for product to render
    ///
    /// # Response Description
    ///
    /// Recursive UI tree: Box, Column, Row, Text, Button, TextField, Spacer, Nil
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Register a custom message renderer
    /// const chatManager = createProductChatManager();
    ///
    /// chatManager.onCustomMessageRenderingRequest(
    ///   ({ messageId, messageType, payload }, render, subscribeActions) => {
    ///     // Render a custom UI
    ///     render({
    ///       Column: {
    ///         modifiers: [{ padding: [8, 12, 8, 12] }],
    ///         props: { horizontalAlignment: "start" },
    ///         children: [
    ///           { Text: {
    ///             modifiers: [],
    ///             props: { style: "headline", color: "textPrimary" },
    ///             children: [{ String: "Custom Poll" }]
    ///           }},
    ///           { Button: {
    ///             modifiers: [],
    ///             props: {
    ///               text: "Vote",
    ///               variant: "primary",
    ///               clickAction: "vote-action"
    ///             },
    ///             children: []
    ///           }}
    ///         ]
    ///       }
    ///     });
    ///
    ///     // Listen for interactions
    ///     subscribeActions((action) => {
    ///       if (action.actionId === "vote-action") {
    ///         handleVote(messageId);
    ///       }
    ///     });
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Host triggers rendering of a custom message
    /// const unsub = container.renderChatCustomMessage(
    ///   {
    ///     messageId: "msg-123",
    ///     messageType: "poll",
    ///     payload: encodedPollData,
    ///   },
    ///   (renderedNode) => {
    ///     // Display the rendered CustomRendererNode tree
    ///     updateChatUI(renderedNode);
    ///   }
    /// );
    /// ```
    ///
    /// # Notes
    ///
    /// This is the only method where roles are reversed. The host initiates and the product responds with rendered UI.
    fn product_chat_custom_message_render_subscribe(
        &self,
        renderer: Box<dyn Fn(CustomMessageRenderRequest) -> CustomRendererNode + Send>,
    ) -> Subscription<()>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Subscribe to, create proofs for, and submit cryptographic statements.
pub trait StatementStore {
    /// Subscribes to statements matching a [`TopicFilter`]. The host pushes
    /// matching signed statements whenever the set changes.
    ///
    /// V0.2: replaces the v0.1 topic-vector signature with a richer
    /// [`TopicFilter`] that supports wildcard positions.
    ///
    /// # Product Function
    ///
    /// `truApi.statementStoreSubscribe(filter, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleStatementStoreSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Subscribe to statements with a topic filter
    /// const topic = new Uint8Array(32);
    /// topic.set([1, 2, 3]); // topic identifier
    ///
    /// // Use null entries as wildcards
    /// const sub = truApi.statementStoreSubscribe(
    ///   { topics: [topic, null] },  // match first topic exactly, any second
    ///   (statements) => {
    ///     for (const stmt of statements) {
    ///       console.log("Statement from:", stmt.proof);
    ///       if (stmt.data) {
    ///         processStatement(stmt.data);
    ///       }
    ///     }
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleStatementStoreSubscribe((filter, send, interrupt) => {
    ///   // filter.topics is an array where null = wildcard
    ///   send(statementStore.queryByFilter(filter));
    ///
    ///   const unsub = statementStore.onChange(filter, (statements) => {
    ///     send(statements);
    ///   });
    ///
    ///   return () => unsub();
    /// });
    /// ```
    fn remote_statement_store_subscribe(
        &self,
        filter: TopicFilter,
    ) -> Subscription<Vec<SignedStatement>>;

    /// Creates a cryptographic proof (signature) for a statement using a product
    /// account's key.
    ///
    /// # Product Function
    ///
    /// `truApi.statementStoreCreateProof(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleStatementStoreCreateProof(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId and a Statement to sign
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a proof for a statement
    /// const result = await truApi.statementStoreCreateProof([
    ///   ["my-product.dot", 0],  // ProductAccountId
    ///   {
    ///     proof: null,
    ///     decryptionKey: null,
    ///     expiry: BigInt(Date.now() + 86400000), // 24 hours
    ///     channel: null,
    ///     topics: [topicHash],
    ///     data: new TextEncoder().encode("my statement"),
    ///   }
    /// ]);
    ///
    /// if (result.isOk) {
    ///   const proof = result.value; // StatementProof
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleStatementStoreCreateProof(
    ///   ([productAccountId, statement], { ok, err }) => {
    ///     const key = getProductKey(productAccountId);
    ///     if (!key) {
    ///       return err({ UnknownAccount: undefined });
    ///     }
    ///     const proof = key.sign(encodeStatement(statement));
    ///     return ok({ Sr25519: { signature: proof, signer: key.publicKey } });
    ///   }
    /// );
    /// ```
    fn remote_statement_store_create_proof(
        &self,
        product_account_id: ProductAccountId,
        statement: Statement,
    ) -> Result<StatementProof, StatementProofError>;

    /// Submits a pre-encoded statement to the statement store and returns
    /// the statement hash on success.
    ///
    /// V0.2: replaces the v0.1 signature that accepted a [`SignedStatement`]
    /// struct with raw SCALE-encoded bytes.
    ///
    /// # Product Function
    ///
    /// `truApi.statementStoreSubmit(encoded)`
    ///
    /// # Host Handler
    ///
    /// `container.handleStatementStoreSubmit(handler)`
    ///
    /// # Request Description
    ///
    /// Raw SCALE-encoded statement bytes
    ///
    /// # Response Description
    ///
    /// Statement hash on success
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Submit a pre-encoded statement (raw SCALE bytes)
    /// const encodedStatement = encodeStatementToScale(signedStatement);
    /// const result = await truApi.statementStoreSubmit(encodedStatement);
    ///
    /// if (result.isOk) {
    ///   console.log("Statement hash:", result.value);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleStatementStoreSubmit((encoded, { ok, err }) => {
    ///   const decoded = decodeStatement(encoded);
    ///   if (!decoded || !verifyProof(decoded)) {
    ///     return err({ GenericError: { reason: "Invalid statement encoding" } });
    ///   }
    ///   const hash = blake2Hash(encoded);
    ///   statementStore.insert(hash, decoded);
    ///   return ok(hash);
    /// });
    /// ```
    fn remote_statement_store_submit(&self, encoded: Bytes) -> Result<String, GenericError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Lookup preimages by hash.
pub trait Preimage {
    /// Subscribes to a preimage by its hash key. The host pushes the value when
    /// it becomes available.
    ///
    /// # Product Function
    ///
    /// `truApi.preimageLookupSubscribe(key, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePreimageLookupSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Subscribe to a preimage
    /// const sub = truApi.preimageLookupSubscribe(
    ///   "0xabcdef1234...",  // hash of the preimage
    ///   (value) => {
    ///     if (value !== null) {
    ///       console.log("Preimage found:", value);
    ///     }
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePreimageLookupSubscribe((key, send, interrupt) => {
    ///   const existing = preimageStore.get(key);
    ///   send(existing ?? null);
    ///
    ///   const unsub = preimageStore.onAvailable(key, (value) => {
    ///     send(value);
    ///   });
    ///
    ///   return () => unsub();
    /// });
    /// ```
    fn remote_preimage_lookup_subscribe(
        &self,
        key: PreimageKey,
    ) -> Subscription<Option<PreimageValue>>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Substrate blockchain RPC access implementing the chainHead v1 JSON-RPC spec over binary protocol.
pub trait ChainInteraction {
    /// Follows the chain head, receiving events about new blocks, finalization,
    /// and operation results. Implements the `chainHead_v1_follow` JSON-RPC
    /// method.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadFollow(params, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChainConnection(factory)`
    ///
    /// # Response Description
    ///
    /// Enum with 12 variants: Initialized, NewBlock, BestBlockChanged, Finalized, OperationBodyDone, OperationCallDone, OperationStorageItems, OperationStorageDone, OperationWaitingForContinue, OperationInaccessible, OperationError, Stop
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Follow chain head events (low-level)
    /// const sub = truApi.chainHeadFollow(
    ///   { genesisHash: polkadotGenesis, withRuntime: true },
    ///   (event) => {
    ///     switch (event.tag) {
    ///       case "Initialized":
    ///         console.log("Finalized:", event.value.finalizedBlockHashes);
    ///         break;
    ///       case "NewBlock":
    ///         console.log("New block:", event.value.blockHash);
    ///         break;
    ///       case "BestBlockChanged":
    ///         console.log("Best:", event.value.bestBlockHash);
    ///         break;
    ///       case "Finalized":
    ///         console.log("Finalized:", event.value.finalizedBlockHashes);
    ///         break;
    ///     }
    ///   }
    /// );
    ///
    /// // Typically used via higher-level abstraction:
    /// // const provider = createPapiProvider(polkadotGenesis);
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Host registers a JSON-RPC provider factory
    /// container.handleChainConnection((genesisHash) => {
    ///   // Return a JsonRpcProvider for the requested chain
    ///   const chain = chains.get(genesisHash);
    ///   if (!chain) return null;
    ///
    ///   return chain.jsonRpcProvider;
    ///   // The chainConnectionManager handles all chain_head_*
    ///   // methods internally via this provider
    /// });
    /// ```
    ///
    /// # Notes
    ///
    /// On the Product Side, typically used via createPapiProvider(genesisHash) from @novasamatech/product-sdk. On the host side, handled via container.handleChainConnection(factory) which manages all chain methods internally.
    fn remote_chain_head_follow(
        &self,
        request: ChainHeadFollowRequest,
    ) -> Subscription<ChainHeadEvent>;

    /// Retrieves a block header by hash within a follow subscription.
    /// Returns the SCALE-encoded block header, or `None`.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadHeader(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Response Description
    ///
    /// SCALE-encoded block header, or null
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainHeadHeader({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hash: blockHash,
    /// });
    ///
    /// if (result.isOk && result.value) {
    ///   const headerBytes = result.value;
    ///   const header = decodeHeader(headerBytes);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_header JSON-RPC call
    /// ```
    fn remote_chain_head_header(
        &self,
        request: ChainHeadBlockRequest,
    ) -> Result<Option<Hex>, GenericError>;

    /// Retrieves a block body. Returns an operation ID; results arrive as
    /// [`ChainHeadEvent::OperationBodyDone`] events on the follow subscription.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadBody(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Response Description
    ///
    /// Started { operationId: OperationId } or LimitReached
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainHeadBody({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hash: blockHash,
    /// });
    ///
    /// if (result.isOk && result.value.tag === "Started") {
    ///   const opId = result.value.value.operationId;
    ///   // Wait for OperationBodyDone event on follow subscription
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_body JSON-RPC call
    /// ```
    fn remote_chain_head_body(
        &self,
        request: ChainHeadBlockRequest,
    ) -> Result<OperationStartedResult, GenericError>;

    /// Queries chain storage. Returns an operation ID; results arrive as
    /// [`ChainHeadEvent::OperationStorageItems`] /
    /// [`ChainHeadEvent::OperationStorageDone`] events.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadStorage(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainHeadStorage({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hash: blockHash,
    ///   items: [
    ///     { key: "0x26aa394eea5630e07c48ae0c9558cef7", type: "Value" }
    ///   ],
    ///   childTrie: null,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_storage JSON-RPC call
    /// ```
    fn remote_chain_head_storage(
        &self,
        request: ChainHeadStorageRequest,
    ) -> Result<OperationStartedResult, GenericError>;

    /// Executes a runtime API call. Returns an operation ID; result arrives as
    /// [`ChainHeadEvent::OperationCallDone`] event.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadCall(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainHeadCall({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hash: blockHash,
    ///   function: "Metadata_metadata",
    ///   callParameters: "0x",
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_call JSON-RPC call
    /// ```
    fn remote_chain_head_call(
        &self,
        request: ChainHeadCallRequest,
    ) -> Result<OperationStartedResult, GenericError>;

    /// Unpins block hashes, allowing the node to discard them.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadUnpin(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// await truApi.chainHeadUnpin({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hashes: [oldBlockHash1, oldBlockHash2],
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_unpin JSON-RPC call
    /// ```
    fn remote_chain_head_unpin(&self, request: ChainHeadUnpinRequest) -> Result<(), GenericError>;

    /// Continues a paused operation (when
    /// [`ChainHeadEvent::OperationWaitingForContinue`] is received).
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadContinue(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // When OperationWaitingForContinue is received:
    /// await truApi.chainHeadContinue({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   operationId: opId,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_continue JSON-RPC call
    /// ```
    fn remote_chain_head_continue(
        &self,
        request: ChainHeadOperationRequest,
    ) -> Result<(), GenericError>;

    /// Stops an in-progress operation.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadStopOperation(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// await truApi.chainHeadStopOperation({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   operationId: opId,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_stopOperation JSON-RPC call
    /// ```
    fn remote_chain_head_stop_operation(
        &self,
        request: ChainHeadOperationRequest,
    ) -> Result<(), GenericError>;

    /// Gets the genesis hash for a chain.
    ///
    /// # Product Function
    ///
    /// `truApi.chainSpecGenesisHash(genesisHash)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainSpecGenesisHash(
    ///   polkadotGenesis
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainSpec_v1_genesisHash JSON-RPC call
    /// ```
    fn remote_chain_spec_genesis_hash(
        &self,
        genesis_hash: GenesisHash,
    ) -> Result<Hex, GenericError>;

    /// Gets the chain name.
    ///
    /// # Product Function
    ///
    /// `truApi.chainSpecChainName(genesisHash)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainSpecChainName(
    ///   polkadotGenesis
    /// );
    /// if (result.isOk) {
    ///   console.log("Chain:", result.value); // "Polkadot"
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainSpec_v1_chainName JSON-RPC call
    /// ```
    fn remote_chain_spec_chain_name(
        &self,
        genesis_hash: GenesisHash,
    ) -> Result<String, GenericError>;

    /// Gets the chain properties as a JSON-encoded string.
    ///
    /// # Product Function
    ///
    /// `truApi.chainSpecProperties(genesisHash)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Response Description
    ///
    /// JSON-encoded chain properties
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainSpecProperties(
    ///   polkadotGenesis
    /// );
    /// if (result.isOk) {
    ///   const props = JSON.parse(result.value);
    ///   console.log("Token:", props.tokenSymbol); // "DOT"
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainSpec_v1_properties JSON-RPC call
    /// ```
    fn remote_chain_spec_properties(
        &self,
        genesis_hash: GenesisHash,
    ) -> Result<String, GenericError>;

    /// Broadcasts a signed transaction to the network.
    /// Returns an operation ID if accepted, `None` if rejected.
    ///
    /// # Product Function
    ///
    /// `truApi.chainTransactionBroadcast(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Response Description
    ///
    /// Operation ID if accepted, null if rejected
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainTransactionBroadcast({
    ///   genesisHash: polkadotGenesis,
    ///   transaction: signedTxHex,
    /// });
    ///
    /// if (result.isOk && result.value) {
    ///   console.log("Broadcasting, op:", result.value);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to transaction_v1_broadcast JSON-RPC call
    /// ```
    fn remote_chain_transaction_broadcast(
        &self,
        request: ChainTransactionBroadcastRequest,
    ) -> Result<Option<String>, GenericError>;

    /// Stops broadcasting a transaction.
    ///
    /// # Product Function
    ///
    /// `truApi.chainTransactionStop(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// await truApi.chainTransactionStop({
    ///   genesisHash: polkadotGenesis,
    ///   operationId: broadcastOpId,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to transaction_v1_stop JSON-RPC call
    /// ```
    fn remote_chain_transaction_stop(
        &self,
        request: ChainTransactionStopRequest,
    ) -> Result<(), GenericError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Payment operations using the Coinage API (RFC 0006).
pub trait Payment {
    /// Subscribes to the user's payment balance. The host prompts the user
    /// for permission to disclose their balance on the first call.
    ///
    /// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).
    ///
    /// # Product Function
    ///
    /// `truApi.paymentBalanceSubscribe(void, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePaymentBalanceSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Subscribe to the user's payment balance
    /// const sub = truApi.paymentBalanceSubscribe(
    ///   undefined,
    ///   (balance) => {
    ///     console.log("Available:", balance.available);
    ///     console.log("Pending:", balance.pending);
    ///     updateBalanceUI(balance);
    ///   }
    /// );
    ///
    /// // Later: clean up
    /// sub.unsubscribe();
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePaymentBalanceSubscribe((_, send, interrupt) => {
    ///   // Prompt user for permission to disclose balance
    ///   const allowed = await requestBalancePermission();
    ///   if (!allowed) {
    ///     throw new PaymentBalanceError("PermissionDenied");
    ///   }
    ///
    ///   // Send initial balance
    ///   send(getUserBalance());
    ///
    ///   const unsub = balanceStore.onChange((balance) => {
    ///     send(balance);
    ///   });
    ///
    ///   return () => unsub();
    /// });
    /// ```
    fn host_payment_balance_subscribe(
        &self,
    ) -> Result<Subscription<PaymentBalance>, PaymentBalanceError>;

    /// Tops up the user's payment balance from a product-controlled funding
    /// source. This operation is always in the user's favour and does not
    /// require user consent.
    ///
    /// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).
    ///
    /// # Product Function
    ///
    /// `truApi.paymentTopUp(amount, source)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePaymentTopUp(handler)`
    ///
    /// # Request Description
    ///
    /// Balance amount and a PaymentTopUpSource (ProductAccount or PrivateKey)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Top up from a product account
    /// const result = await truApi.paymentTopUp(
    ///   1000000n,  // amount in smallest unit
    ///   { ProductAccount: 0 }  // derivation index
    /// );
    ///
    /// if (result.isErr) {
    ///   if (result.error.tag === "InsufficientFunds") {
    ///     console.error("Source account has insufficient funds");
    ///   }
    /// }
    ///
    /// // Top up from a one-time private key
    /// const result2 = await truApi.paymentTopUp(
    ///   5000000n,
    ///   { PrivateKey: privateKeyBytes }  // 32-byte Ed25519 private key
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePaymentTopUp(([amount, source], { ok, err }) => {
    ///   if (source.tag === "ProductAccount") {
    ///     const account = getProductAccount(source.value);
    ///     if (account.balance < amount) {
    ///       return err({ InsufficientFunds: undefined });
    ///     }
    ///     transferToUserBalance(account, amount);
    ///   } else if (source.tag === "PrivateKey") {
    ///     const account = accountFromPrivateKey(source.value);
    ///     if (!account) {
    ///       return err({ InvalidSource: undefined });
    ///     }
    ///     transferToUserBalance(account, amount);
    ///   }
    ///   return ok(undefined);
    /// });
    /// ```
    fn host_payment_top_up(
        &self,
        amount: Balance,
        source: PaymentTopUpSource,
    ) -> Result<(), PaymentTopUpError>;

    /// Requests a payment from the user's available balance to `destination`.
    /// The host prompts the user to authorize. Returns a [`PaymentReceipt`]
    /// whose [`PaymentId`] can be tracked via
    /// [`host_payment_status_subscribe`](Payment::host_payment_status_subscribe).
    ///
    /// A successful response means the user authorized the payment and the
    /// host accepted it for processing, **not** that it has settled.
    ///
    /// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).
    ///
    /// # Product Function
    ///
    /// `truApi.paymentRequest(amount, destination)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePaymentRequest(handler)`
    ///
    /// # Request Description
    ///
    /// Balance amount and destination AccountId (32 bytes)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Request payment from the user
    /// const result = await truApi.paymentRequest(
    ///   500000n,  // amount
    ///   destinationAccountId  // 32-byte AccountId
    /// );
    ///
    /// if (result.isOk) {
    ///   const { id } = result.value;
    ///   console.log("Payment accepted, tracking ID:", id);
    ///   // Track the payment status
    ///   trackPayment(id);
    /// } else if (result.error.tag === "Denied") {
    ///   console.log("User denied the payment");
    /// } else if (result.error.tag === "InsufficientBalance") {
    ///   console.log("User does not have enough balance");
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePaymentRequest(([amount, destination], { ok, err }) => {
    ///   if (getUserBalance().available < amount) {
    ///     return err({ InsufficientBalance: undefined });
    ///   }
    ///
    ///   const approved = await showPaymentDialog(amount, destination);
    ///   if (!approved) {
    ///     return err({ Denied: undefined });
    ///   }
    ///
    ///   const paymentId = initiatePayment(amount, destination);
    ///   return ok({ id: paymentId });
    /// });
    /// ```
    fn host_payment_request(
        &self,
        amount: Balance,
        destination: AccountId,
    ) -> Result<PaymentReceipt, PaymentRequestError>;

    /// Subscribes to status updates for a previously requested payment.
    /// Emits status changes until a terminal state (`Completed` or `Failed`).
    ///
    /// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).
    ///
    /// # Product Function
    ///
    /// `truApi.paymentStatusSubscribe(paymentId, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePaymentStatusSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Track a payment's lifecycle
    /// const sub = truApi.paymentStatusSubscribe(
    ///   paymentId,
    ///   (status) => {
    ///     switch (status.tag ?? status) {
    ///       case "Processing":
    ///         showSpinner("Payment processing...");
    ///         break;
    ///       case "Completed":
    ///         showSuccess("Payment completed!");
    ///         sub.unsubscribe();
    ///         break;
    ///       case "Failed":
    ///         showError("Payment failed:", status.value);
    ///         sub.unsubscribe();
    ///         break;
    ///     }
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePaymentStatusSubscribe((paymentId, send, interrupt) => {
    ///   const payment = paymentStore.get(paymentId);
    ///   if (!payment) {
    ///     throw new PaymentStatusError("PaymentNotFound");
    ///   }
    ///
    ///   // Send current status
    ///   send(payment.status);
    ///
    ///   const unsub = paymentStore.onStatusChange(paymentId, (status) => {
    ///     send(status);
    ///   });
    ///
    ///   return () => unsub();
    /// });
    /// ```
    fn host_payment_status_subscribe(
        &self,
        payment_id: PaymentId,
    ) -> Result<Subscription<PaymentStatus>, PaymentStatusError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Deterministic entropy derivation (RFC 0007).
pub trait EntropyDerivation {
    /// Derives 32 bytes of deterministic entropy scoped to the calling product
    /// and the provided `key`. Uses a three-layer BLAKE2b-256 keyed hashing
    /// scheme over the user's root BIP-39 entropy.
    ///
    /// `key` is an arbitrary value (up to 32 bytes) chosen by the caller; the
    /// host does not assign any semantic meaning to it.
    ///
    /// The same root account + product + key always yields the same
    /// [`Entropy`] on every conforming host.
    ///
    /// See [RFC 0007](https://github.com/paritytech/triangle-js-sdks/pull/95).
    ///
    /// # Product Function
    ///
    /// `truApi.deriveEntropy(key)`
    ///
    /// # Host Handler
    ///
    /// `container.handleDeriveEntropy(handler)`
    ///
    /// # Request Description
    ///
    /// Arbitrary key bytes (up to 32 bytes) chosen by the caller
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Derive deterministic entropy for a specific purpose
    /// const key = new TextEncoder().encode("my-secret-seed");
    /// const result = await truApi.deriveEntropy(key);
    ///
    /// if (result.isOk) {
    ///   const entropy = result.value; // 32 bytes, deterministic
    ///   // Use entropy to seed a PRNG, derive keys, etc.
    ///   const derivedKey = await crypto.subtle.importKey(
    ///     "raw", entropy, "HKDF", false, ["deriveBits"]
    ///   );
    /// }
    ///
    /// // Same key always produces the same entropy for the same user+product
    /// const result2 = await truApi.deriveEntropy(key);
    /// // result2.value === result.value (byte-for-byte identical)
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleDeriveEntropy((key, { ok, err }) => {
    ///   // Three-layer BLAKE2b-256 keyed hashing:
    ///   // 1. Layer 1: BLAKE2b(rootEntropy, "truapi-entropy-v1")
    ///   // 2. Layer 2: BLAKE2b(layer1, productDotNsId)
    ///   // 3. Layer 3: BLAKE2b(layer2, key)
    ///   const entropy = deriveThreeLayerEntropy(
    ///     currentUser.rootEntropy,
    ///     currentProduct.dotNsId,
    ///     key
    ///   );
    ///   return ok(entropy);
    /// });
    /// ```
    fn host_derive_entropy(&self, key: Vec<u8>) -> Result<Entropy, DeriveEntropyError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// The combined TrUAPI v0.2 interface.
pub trait TrUApi:
    TrUApiCalls
    + Permissions
    + LocalStorage
    + AccountManagement
    + Signing
    + Chat
    + StatementStore
    + Preimage
    + ChainInteraction
    + Payment
    + EntropyDerivation
{
}
