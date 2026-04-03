//! TrUAPI Protocol v0.1 -- trait and type definitions.
//!
//! This module defines the TrUAPI v0.1 traits and all data types used in their signatures.
//! The three communication patterns are:
//!
//! - **Request-response**: product calls host, host returns a result.
//! - **Subscription**: product subscribes, host pushes values as a [`Subscription`] stream.
//! - **Reverse-subscription**: host initiates, product responds (only used for custom chat
//!   message rendering).

use crate::Subscription;

mod account;
mod chain_interaction;
mod chat;
mod common;
mod custom_renderer;
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
pub use preimage::*;
pub use signing::*;
pub use statement_store::*;
pub use storage::*;
pub use transaction::*;

// ─── TrUApiCalls trait ─────────────────────────────────────────────────────

/// General-purpose TrUAPI methods for feature detection, navigation, notifications, and permissions.
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

// ─── Permissions trait ─────────────────────────────────────────────────────

/// Device and remote permission requests for camera, microphone, HTTP, and transaction access.
pub trait Permissions {
    /// Requests access to a device capability (camera, microphone, bluetooth,
    /// location).
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
    /// Status enum: Camera | Microphone | Bluetooth | Location
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Request camera access
    /// const granted = await truApi.devicePermission("Camera");
    ///
    /// if (granted.isOk && granted.value) {
    ///   // Camera access granted, start video stream
    ///   startCamera();
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleDevicePermission((permission, { ok, err }) => {
    ///   // Show permission dialog to user
    ///   const granted = await showPermissionDialog(permission);
    ///   return ok(granted);
    /// });
    /// ```
    fn host_device_permission(
        &self,
        permission: DevicePermissionRequest,
    ) -> Result<bool, GenericError>;

    /// Requests permission for a remote operation (external HTTP request or
    /// transaction submission).
    ///
    /// # Product Function
    ///
    /// `truApi.permission(request)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePermission(handler)`
    ///
    /// # Request Description
    ///
    /// Enum: ExternalRequest(str) | TransactionSubmit
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Request permission to fetch from an external API
    /// const allowed = await truApi.permission({
    ///   ExternalRequest: "https://api.coingecko.com/api/v3/simple/price"
    /// });
    ///
    /// if (allowed.isOk && allowed.value) {
    ///   const price = await fetch("https://api.coingecko.com/...");
    /// }
    ///
    /// // Request permission to submit transactions
    /// const txAllowed = await truApi.permission({
    ///   TransactionSubmit: undefined
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePermission((request, { ok, err }) => {
    ///   if (request.tag === "ExternalRequest") {
    ///     const allowed = isUrlAllowed(request.value);
    ///     return ok(allowed);
    ///   }
    ///   if (request.tag === "TransactionSubmit") {
    ///     return ok(userHasApprovedTxSubmission);
    ///   }
    ///   return ok(false);
    /// });
    /// ```
    fn remote_permission(&self, request: RemotePermissionRequest) -> Result<bool, GenericError>;
}

// ─── LocalStorage trait ────────────────────────────────────────────────────

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

// ─── AccountManagement trait ───────────────────────────────────────────────

/// Product-specific account derivation, alias retrieval, ring VRF proofs, and connection status.
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
}

// ─── Signing trait ─────────────────────────────────────────────────────────

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
    ///   address: "5GrwvaEF5...",
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
    ///   // Show signing modal to user
    ///   const userApproved = await showSigningDialog(payload);
    ///   if (!userApproved) {
    ///     return err({ Rejected: undefined });
    ///   }
    ///   const signature = await signer.sign(payload);
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
    ///   address: "5GrwvaEF5...",
    ///   data: { Payload: "Please sign this message to verify ownership" }
    /// });
    ///
    /// // Or sign raw bytes
    /// const result2 = await truApi.signRaw({
    ///   address: "5GrwvaEF5...",
    ///   data: { Bytes: new Uint8Array([1, 2, 3]) }
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleSignRaw((payload, { ok, err }) => {
    ///   const userApproved = await showRawSigningDialog(payload);
    ///   if (!userApproved) {
    ///     return err({ Rejected: undefined });
    ///   }
    ///   const signature = await signer.signRaw(
    ///     payload.address, payload.data
    ///   );
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

// ─── Chat trait ────────────────────────────────────────────────────────────

/// Chat room management, bot registration, message posting, and custom message rendering.
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

// ─── StatementStore trait ──────────────────────────────────────────────────

/// Subscribe to, create proofs for, and submit cryptographic statements.
pub trait StatementStore {
    /// Subscribes to statements matching a set of topics. The host pushes
    /// matching signed statements whenever the set changes.
    ///
    /// # Product Function
    ///
    /// `truApi.statementStoreSubscribe(topics, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleStatementStoreSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Subscribe to statements for specific topics
    /// const topic = new Uint8Array(32);
    /// topic.set([1, 2, 3]); // topic identifier
    ///
    /// const sub = truApi.statementStoreSubscribe(
    ///   [topic],
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
    /// container.handleStatementStoreSubscribe((topics, send, interrupt) => {
    ///   // Send matching statements
    ///   send(statementStore.queryByTopics(topics));
    ///
    ///   const unsub = statementStore.onChange(topics, (statements) => {
    ///     send(statements);
    ///   });
    ///
    ///   return () => unsub();
    /// });
    /// ```
    fn remote_statement_store_subscribe(
        &self,
        topics: Vec<Topic>,
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

    /// Submits a signed statement to the statement store.
    ///
    /// # Product Function
    ///
    /// `truApi.statementStoreSubmit(statement)`
    ///
    /// # Host Handler
    ///
    /// `container.handleStatementStoreSubmit(handler)`
    ///
    /// # Request Description
    ///
    /// See SignedStatement type for fields
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Submit a signed statement
    /// const result = await truApi.statementStoreSubmit({
    ///   proof: { Sr25519: { signature: sig, signer: pubKey } },
    ///   decryptionKey: null,
    ///   expiry: BigInt(Date.now() + 86400000),
    ///   channel: null,
    ///   topics: [topicHash],
    ///   data: encodedData,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleStatementStoreSubmit((statement, { ok, err }) => {
    ///   if (!verifyProof(statement.proof, statement)) {
    ///     return err({ GenericError: { reason: "Invalid proof" } });
    ///   }
    ///   statementStore.insert(statement);
    ///   return ok(undefined);
    /// });
    /// ```
    fn remote_statement_store_submit(&self, statement: SignedStatement)
        -> Result<(), GenericError>;
}

// ─── Preimage trait ────────────────────────────────────────────────────────

/// Lookup and submit preimages by hash.
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

    /// Submits a preimage value and receives its hash key back.
    ///
    /// # Product Function
    ///
    /// `truApi.preimageSubmit(value)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePreimageSubmit(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Submit a preimage
    /// const data = new TextEncoder().encode("my preimage data");
    /// const result = await truApi.preimageSubmit(data);
    ///
    /// if (result.isOk) {
    ///   console.log("Preimage key:", result.value); // hash
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePreimageSubmit((value, { ok, err }) => {
    ///   const key = blake2Hash(value);
    ///   preimageStore.set(key, value);
    ///   return ok(key);
    /// });
    /// ```
    fn remote_preimage_submit(
        &self,
        value: PreimageValue,
    ) -> Result<PreimageKey, PreimageSubmitError>;
}

// ─── ChainInteraction trait ────────────────────────────────────────────────

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

// ─── Combined TrUApi trait ─────────────────────────────────────────────────

/// The combined TrUAPI v0.1 interface. A host implements this by implementing all group traits.
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
{
}
