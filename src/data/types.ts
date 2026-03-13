export interface DataType {
  id: string;
  name: string;
  category: string;
  source?: string;
  definition: string;
  description: string;
  fields?: { name: string; type: string; description: string }[];
  variants?: { name: string; type: string; description: string }[];
}

export interface MethodDef {
  id: string;
  name: string;
  group: string;
  groupId: string;
  pattern: 'request-response' | 'subscription' | 'reverse-subscription';
  description: string;
  productFunction: string;
  hostHandler: string;
  request: string;
  response: string;
  requestDescription?: string;
  responseDescription?: string;
  errorType?: string;
  errorVariants?: string[];
  productExample: string;
  hostExample: string;
  notes?: string;
}

export interface GroupDef {
  id: string;
  name: string;
  description: string;
  methods: string[];
}

export const groups: GroupDef[] = [
  {
    id: 'host-calls',
    name: 'Host Calls',
    description: 'General-purpose host methods for feature detection, navigation, notifications, and permissions.',
    methods: ['host_feature_supported', 'host_navigate_to', 'host_push_notification'],
  },
  {
    id: 'permissions',
    name: 'Permissions',
    description: 'Device and remote permission requests for camera, microphone, HTTP, and transaction access.',
    methods: ['host_device_permission', 'remote_permission'],
  },
  {
    id: 'local-storage',
    name: 'Local Storage',
    description: 'Scoped key-value storage. The host namespaces keys so different products cannot read each other\'s data.',
    methods: ['host_local_storage_read', 'host_local_storage_write', 'host_local_storage_clear'],
  },
  {
    id: 'account-management',
    name: 'Account Management',
    description: 'Product-specific account derivation, alias retrieval, ring VRF proofs, and connection status.',
    methods: ['host_account_get', 'host_account_get_alias', 'host_account_create_proof', 'host_get_non_product_accounts', 'host_account_connection_status_subscribe'],
  },
  {
    id: 'signing',
    name: 'Signing',
    description: 'Transaction payload signing, raw message signing, and full transaction creation.',
    methods: ['host_sign_payload', 'host_sign_raw', 'host_create_transaction', 'host_create_transaction_with_non_product_account'],
  },
  {
    id: 'chat',
    name: 'Chat',
    description: 'Chat room management, bot registration, message posting, and custom message rendering.',
    methods: ['host_chat_create_room', 'host_chat_register_bot', 'host_chat_post_message', 'host_chat_list_subscribe', 'host_chat_action_subscribe', 'product_chat_custom_message_render_subscribe'],
  },
  {
    id: 'statement-store',
    name: 'Statement Store',
    description: 'Subscribe to, create proofs for, and submit cryptographic statements.',
    methods: ['remote_statement_store_subscribe', 'remote_statement_store_create_proof', 'remote_statement_store_submit'],
  },
  {
    id: 'preimage',
    name: 'Preimage',
    description: 'Lookup and submit preimages by hash.',
    methods: ['remote_preimage_lookup_subscribe', 'remote_preimage_submit'],
  },
  {
    id: 'chain-interaction',
    name: 'Chain Interaction',
    description: 'Substrate blockchain RPC access implementing the chainHead v1 JSON-RPC spec over binary protocol.',
    methods: ['remote_chain_head_follow', 'remote_chain_head_header', 'remote_chain_head_body', 'remote_chain_head_storage', 'remote_chain_head_call', 'remote_chain_head_unpin', 'remote_chain_head_continue', 'remote_chain_head_stop_operation', 'remote_chain_spec_genesis_hash', 'remote_chain_spec_chain_name', 'remote_chain_spec_properties', 'remote_chain_transaction_broadcast', 'remote_chain_transaction_stop'],
  },
];

export const methods: MethodDef[] = [
  // Group 1: Host Calls
  {
    id: 'host_feature_supported',
    name: 'host_feature_supported',
    group: 'Host Calls',
    groupId: 'host-calls',
    pattern: 'request-response',
    description: 'Queries whether the host supports a specific feature. Currently only the Chain variant exists, carrying a genesis hash to check whether a specific blockchain is available.',
    productFunction: 'hostApi.featureSupported(feature)',
    hostHandler: 'container.handleFeatureSupported(handler)',
    request: 'Feature',
    response: 'Result(bool, GenericError)',
    requestDescription: 'Feature enum — Chain(GenesisHash)',
    productExample: `// Check if Polkadot is supported
const polkadotGenesis = "0x91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3";
const result = await hostApi.featureSupported({
  Chain: polkadotGenesis
});

if (result.isOk) {
  console.log("Polkadot supported:", result.value);
}`,
    hostExample: `container.handleFeatureSupported((feature, { ok, err }) => {
  if (feature.tag === "Chain") {
    const supported = supportedChains.has(feature.value);
    return ok(supported);
  }
  return ok(false);
});`,
  },
  {
    id: 'host_navigate_to',
    name: 'host_navigate_to',
    group: 'Host Calls',
    groupId: 'host-calls',
    pattern: 'request-response',
    description: 'Requests the host to open a URL, typically in a new browser tab.',
    productFunction: 'hostApi.navigateTo(url)',
    hostHandler: 'container.handleNavigateTo(handler)',
    request: 'str',
    response: 'Result(void, NavigateToErr)',
    requestDescription: 'The URL to open',
    errorType: 'NavigateToErr',
    errorVariants: ['PermissionDenied', 'Unknown({ reason: str })'],
    productExample: `// Open an external link
const result = await hostApi.navigateTo(
  "https://polkadot.network"
);

if (result.isErr) {
  console.error("Navigation failed:", result.error);
}`,
    hostExample: `container.handleNavigateTo((url, { ok, err }) => {
  try {
    window.open(url, "_blank");
    return ok(undefined);
  } catch (e) {
    return err({ PermissionDenied: undefined });
  }
});`,
  },
  {
    id: 'host_push_notification',
    name: 'host_push_notification',
    group: 'Host Calls',
    groupId: 'host-calls',
    pattern: 'request-response',
    description: 'Sends a push notification to the user via the host.',
    productFunction: 'hostApi.pushNotification(notification)',
    hostHandler: 'container.handlePushNotification(handler)',
    request: 'PushNotification',
    response: 'Result(void, GenericError)',
    requestDescription: 'See PushNotification type for fields',
    productExample: `// Send a notification with a deeplink
const result = await hostApi.pushNotification({
  text: "Your transaction was confirmed!",
  deeplink: "myapp://tx/0xabc123"
});`,
    hostExample: `container.handlePushNotification((notification, { ok, err }) => {
  showSystemNotification(notification.text, {
    onclick: () => {
      if (notification.deeplink) {
        navigate(notification.deeplink);
      }
    }
  });
  return ok(undefined);
});`,
  },
  // Group 2: Permissions
  {
    id: 'host_device_permission',
    name: 'host_device_permission',
    group: 'Permissions',
    groupId: 'permissions',
    pattern: 'request-response',
    description: 'Requests access to a device capability (camera, microphone, bluetooth, location).',
    productFunction: 'hostApi.devicePermission(permission)',
    hostHandler: 'container.handleDevicePermission(handler)',
    request: 'DevicePermissionRequest',
    response: 'Result(bool, GenericError)',
    requestDescription: 'Status enum: Camera | Microphone | Bluetooth | Location',
    productExample: `// Request camera access
const granted = await hostApi.devicePermission("Camera");

if (granted.isOk && granted.value) {
  // Camera access granted, start video stream
  startCamera();
}`,
    hostExample: `container.handleDevicePermission((permission, { ok, err }) => {
  // Show permission dialog to user
  const granted = await showPermissionDialog(permission);
  return ok(granted);
});`,
  },
  {
    id: 'remote_permission',
    name: 'remote_permission',
    group: 'Permissions',
    groupId: 'permissions',
    pattern: 'request-response',
    description: 'Requests permission for a remote operation (external HTTP request or transaction submission).',
    productFunction: 'hostApi.permission(request)',
    hostHandler: 'container.handlePermission(handler)',
    request: 'RemotePermissionRequest',
    response: 'Result(bool, GenericError)',
    requestDescription: 'Enum: ExternalRequest(str) | TransactionSubmit',
    productExample: `// Request permission to fetch from an external API
const allowed = await hostApi.permission({
  ExternalRequest: "https://api.coingecko.com/api/v3/simple/price"
});

if (allowed.isOk && allowed.value) {
  const price = await fetch("https://api.coingecko.com/...");
}

// Request permission to submit transactions
const txAllowed = await hostApi.permission({
  TransactionSubmit: undefined
});`,
    hostExample: `container.handlePermission((request, { ok, err }) => {
  if (request.tag === "ExternalRequest") {
    const allowed = isUrlAllowed(request.value);
    return ok(allowed);
  }
  if (request.tag === "TransactionSubmit") {
    return ok(userHasApprovedTxSubmission);
  }
  return ok(false);
});`,
  },
  // Group 3: Local Storage
  {
    id: 'host_local_storage_read',
    name: 'host_local_storage_read',
    group: 'Local Storage',
    groupId: 'local-storage',
    pattern: 'request-response',
    description: 'Reads a value from the scoped key-value store.',
    productFunction: 'hostApi.localStorageRead(key)',
    hostHandler: 'container.handleLocalStorageRead(handler)',
    request: 'StorageKey',
    response: 'Result(Option(StorageValue), StorageErr)',
    errorType: 'StorageErr',
    errorVariants: ['Full', 'Unknown({ reason: str })'],
    productExample: `// Read a stored preference
const result = await hostApi.localStorageRead("user-theme");

if (result.isOk && result.value !== null) {
  const theme = new TextDecoder().decode(result.value);
  applyTheme(theme);
}`,
    hostExample: `container.handleLocalStorageRead((key, { ok, err }) => {
  const namespacedKey = \`\${productId}:\${key}\`;
  const value = localStorage.getItem(namespacedKey);
  return ok(value ? new TextEncoder().encode(value) : null);
});`,
  },
  {
    id: 'host_local_storage_write',
    name: 'host_local_storage_write',
    group: 'Local Storage',
    groupId: 'local-storage',
    pattern: 'request-response',
    description: 'Writes a value to the scoped key-value store.',
    productFunction: 'hostApi.localStorageWrite([key, value])',
    hostHandler: 'container.handleLocalStorageWrite(handler)',
    request: 'Tuple(StorageKey, StorageValue)',
    response: 'Result(void, StorageErr)',
    errorType: 'StorageErr',
    errorVariants: ['Full', 'Unknown({ reason: str })'],
    productExample: `// Store a user preference
const theme = new TextEncoder().encode("dark");
const result = await hostApi.localStorageWrite([
  "user-theme",
  theme
]);

if (result.isErr) {
  console.error("Storage write failed:", result.error);
}`,
    hostExample: `container.handleLocalStorageWrite(([key, value], { ok, err }) => {
  const namespacedKey = \`\${productId}:\${key}\`;
  try {
    localStorage.setItem(namespacedKey, new TextDecoder().decode(value));
    return ok(undefined);
  } catch (e) {
    return err({ Full: undefined });
  }
});`,
  },
  {
    id: 'host_local_storage_clear',
    name: 'host_local_storage_clear',
    group: 'Local Storage',
    groupId: 'local-storage',
    pattern: 'request-response',
    description: 'Clears a value from the scoped key-value store.',
    productFunction: 'hostApi.localStorageClear(key)',
    hostHandler: 'container.handleLocalStorageClear(handler)',
    request: 'StorageKey',
    response: 'Result(void, StorageErr)',
    errorType: 'StorageErr',
    errorVariants: ['Full', 'Unknown({ reason: str })'],
    productExample: `// Clear stored data
const result = await hostApi.localStorageClear("user-theme");`,
    hostExample: `container.handleLocalStorageClear((key, { ok, err }) => {
  const namespacedKey = \`\${productId}:\${key}\`;
  localStorage.removeItem(namespacedKey);
  return ok(undefined);
});`,
  },
  // Group 4: Account Management
  {
    id: 'host_account_get',
    name: 'host_account_get',
    group: 'Account Management',
    groupId: 'account-management',
    pattern: 'request-response',
    description: 'Retrieves a product-specific derived account. The product provides a product identifier and derivation index; the host derives a unique public key for that combination.',
    productFunction: 'hostApi.accountGet(productAccountId)',
    hostHandler: 'container.handleAccountGet(handler)',
    request: 'ProductAccountId',
    response: 'Result(Account, RequestCredentialsErr)',
    requestDescription: 'ProductAccountId is a Tuple(DotNsIdentifier, DerivationIndex)',
    errorType: 'RequestCredentialsErr',
    errorVariants: ['NotConnected', 'Rejected', 'DomainNotValid', 'Unknown({ reason: str })'],
    productExample: `// Get the product account for "my-product" with index 0
const result = await hostApi.accountGet([
  "my-product.dot",  // DotNS identifier
  0               // derivation index
]);

if (result.isOk) {
  const { publicKey, name } = result.value;
  console.log("Account:", name ?? "unnamed");
  console.log("Key:", toHex(publicKey));
}`,
    hostExample: `container.handleAccountGet(([dotNsId, derivationIndex], { ok, err }) => {
  if (!currentUser) {
    return err({ NotConnected: undefined });
  }
  const account = deriveProductAccount(
    currentUser, dotNsId, derivationIndex
  );
  return ok({
    publicKey: account.publicKey,
    name: account.displayName ?? null,
  });
});`,
  },
  {
    id: 'host_account_get_alias',
    name: 'host_account_get_alias',
    group: 'Account Management',
    groupId: 'account-management',
    pattern: 'request-response',
    description: 'Retrieves a contextual alias (ring VRF based) for a product account.',
    productFunction: 'hostApi.accountGetAlias(productAccountId)',
    hostHandler: 'container.handleAccountGetAlias(handler)',
    request: 'ProductAccountId',
    response: 'Result(ContextualAlias, RequestCredentialsErr)',
    requestDescription: 'ProductAccountId is a Tuple(DotNsIdentifier, DerivationIndex)',
    errorType: 'RequestCredentialsErr',
    errorVariants: ['NotConnected', 'Rejected', 'DomainNotValid', 'Unknown({ reason: str })'],
    productExample: `// Get a contextual alias for privacy-preserving identity
const result = await hostApi.accountGetAlias([
  "my-product.dot",
  0
]);

if (result.isOk) {
  const { context, alias } = result.value;
  // Use alias for anonymous interactions
}`,
    hostExample: `container.handleAccountGetAlias(([dotNsId, derivationIndex], { ok, err }) => {
  if (!currentUser) {
    return err({ NotConnected: undefined });
  }
  const alias = computeContextualAlias(
    currentUser, dotNsId, derivationIndex
  );
  return ok(alias);
});`,
  },
  {
    id: 'host_account_create_proof',
    name: 'host_account_create_proof',
    group: 'Account Management',
    groupId: 'account-management',
    pattern: 'request-response',
    description: 'Creates a ring VRF proof for a product account against a specific ring.',
    productFunction: 'hostApi.accountCreateProof(params)',
    hostHandler: 'container.handleAccountCreateProof(handler)',
    request: 'Tuple(ProductAccountId, RingLocation, Bytes)',
    response: 'Result(RingVrfProof, CreateProofErr)',
    requestDescription: 'ProductAccountId, RingLocation, and context bytes',
    errorType: 'CreateProofErr',
    errorVariants: ['RingNotFound', 'Rejected', 'Unknown({ reason: str })'],
    productExample: `// Create a ring VRF proof
const result = await hostApi.accountCreateProof([
  ["my-product.dot", 0],          // ProductAccountId
  {                              // RingLocation
    genesisHash: polkadotGenesis,
    ringRootHash: "0xabcdef...",
    hints: { palletInstance: 42 },
  },
  contextBytes                   // Bytes - context data
]);

if (result.isOk) {
  const proof = result.value; // RingVrfProof
}`,
    hostExample: `container.handleAccountCreateProof(
  ([productAccountId, ringLocation, context], { ok, err }) => {
    const proof = ringVrf.createProof(
      productAccountId, ringLocation, context
    );
    if (!proof) {
      return err({ RingNotFound: undefined });
    }
    return ok(proof);
  }
);`,
  },
  {
    id: 'host_get_non_product_accounts',
    name: 'host_get_non_product_accounts',
    group: 'Account Management',
    groupId: 'account-management',
    pattern: 'request-response',
    description: 'Retrieves the user\'s non-product accounts (e.g., their main wallet account, not derived per-product).',
    productFunction: 'hostApi.getNonProductAccounts()',
    hostHandler: 'container.handleGetNonProductAccounts(handler)',
    request: 'void',
    response: 'Result(Vector(Account), RequestCredentialsErr)',
    errorType: 'RequestCredentialsErr',
    errorVariants: ['NotConnected', 'Rejected', 'DomainNotValid', 'Unknown({ reason: str })'],
    productExample: `// Get the user's wallet accounts
const result = await hostApi.getNonProductAccounts();

if (result.isOk) {
  for (const account of result.value) {
    console.log(account.name, toHex(account.publicKey));
  }
}`,
    hostExample: `container.handleGetNonProductAccounts((_, { ok, err }) => {
  if (!currentUser) {
    return err({ NotConnected: undefined });
  }
  return ok(currentUser.walletAccounts.map(a => ({
    publicKey: a.publicKey,
    name: a.displayName ?? null,
  })));
});`,
  },
  {
    id: 'host_account_connection_status_subscribe',
    name: 'host_account_connection_status_subscribe',
    group: 'Account Management',
    groupId: 'account-management',
    pattern: 'subscription',
    description: 'Subscribes to changes in the user\'s authentication state. The host pushes "connected" or "disconnected" whenever the auth state changes.',
    productFunction: 'hostApi.accountConnectionStatusSubscribe(void, callback)',
    hostHandler: 'container.handleAccountConnectionStatusSubscribe(handler)',
    request: 'void',
    response: 'AccountConnectionStatus',
    responseDescription: 'Status enum: "disconnected" | "connected"',
    productExample: `// Watch for authentication changes
const sub = hostApi.accountConnectionStatusSubscribe(
  undefined,
  (status) => {
    if (status === "connected") {
      showWalletUI();
    } else {
      showConnectButton();
    }
  }
);

// Later: clean up
sub.unsubscribe();`,
    hostExample: `container.handleAccountConnectionStatusSubscribe(
  (params, send, interrupt) => {
    // Send initial status
    send(currentUser ? "connected" : "disconnected");

    // Watch for changes
    const unsub = authStore.onChange((user) => {
      send(user ? "connected" : "disconnected");
    });

    return () => unsub(); // cleanup
  }
);`,
  },
  // Group 5: Signing
  {
    id: 'host_sign_payload',
    name: 'host_sign_payload',
    group: 'Signing',
    groupId: 'signing',
    pattern: 'request-response',
    description: 'Requests the host to sign a Substrate transaction payload. The host typically shows a confirmation modal to the user.',
    productFunction: 'hostApi.signPayload(payload)',
    hostHandler: 'container.handleSignPayload(handler)',
    request: 'SigningPayload',
    response: 'Result(SigningResult, SigningErr)',
    requestDescription: 'See SigningPayload type for all fields',
    errorType: 'SigningErr',
    errorVariants: ['FailedToDecode', 'Rejected', 'PermissionDenied', 'Unknown({ reason: str })'],
    productExample: `// Sign a Substrate extrinsic payload
const result = await hostApi.signPayload({
  address: "5GrwvaEF5...",
  blockHash: "0xabc...",
  blockNumber: "0x01",
  era: "0x6502",
  genesisHash: polkadotGenesis,
  method: "0x0500...",    // encoded call data
  nonce: "0x00",
  specVersion: "0x01",
  tip: "0x00",
  transactionVersion: "0x01",
  signedExtensions: ["CheckSpecVersion", "CheckTxVersion"],
  version: 4,
  withSignedTransaction: true,
});

if (result.isOk) {
  const { signature, signedTransaction } = result.value;
}`,
    hostExample: `container.handleSignPayload((payload, { ok, err }) => {
  // Show signing modal to user
  const userApproved = await showSigningDialog(payload);
  if (!userApproved) {
    return err({ Rejected: undefined });
  }
  const signature = await signer.sign(payload);
  return ok({
    signature,
    signedTransaction: null,
  });
});`,
  },
  {
    id: 'host_sign_raw',
    name: 'host_sign_raw',
    group: 'Signing',
    groupId: 'signing',
    pattern: 'request-response',
    description: 'Requests the host to sign a raw message (not a transaction).',
    productFunction: 'hostApi.signRaw(payload)',
    hostHandler: 'container.handleSignRaw(handler)',
    request: 'SigningRawPayload',
    response: 'Result(SigningResult, SigningErr)',
    requestDescription: 'See SigningRawPayload type for fields',
    errorType: 'SigningErr',
    errorVariants: ['FailedToDecode', 'Rejected', 'PermissionDenied', 'Unknown({ reason: str })'],
    productExample: `// Sign a raw message
const result = await hostApi.signRaw({
  address: "5GrwvaEF5...",
  data: { Payload: "Please sign this message to verify ownership" }
});

// Or sign raw bytes
const result2 = await hostApi.signRaw({
  address: "5GrwvaEF5...",
  data: { Bytes: new Uint8Array([1, 2, 3]) }
});`,
    hostExample: `container.handleSignRaw((payload, { ok, err }) => {
  const userApproved = await showRawSigningDialog(payload);
  if (!userApproved) {
    return err({ Rejected: undefined });
  }
  const signature = await signer.signRaw(
    payload.address, payload.data
  );
  return ok({ signature, signedTransaction: null });
});`,
  },
  {
    id: 'host_create_transaction',
    name: 'host_create_transaction',
    group: 'Signing',
    groupId: 'signing',
    pattern: 'request-response',
    description: 'Requests the host to create and sign a full transaction from a structured payload, using a product-derived account.',
    productFunction: 'hostApi.createTransaction(params)',
    hostHandler: 'container.handleCreateTransaction(handler)',
    request: 'Tuple(ProductAccountId, VersionedTxPayload)',
    response: 'Result(Bytes, CreateTransactionErr)',
    requestDescription: 'ProductAccountId and a VersionedTxPayload',
    responseDescription: 'The signed transaction bytes',
    errorType: 'CreateTransactionErr',
    errorVariants: ['FailedToDecode', 'Rejected', 'NotSupported(str)', 'PermissionDenied', 'Unknown({ reason: str })'],
    productExample: `// Create a signed transaction using product account
const result = await hostApi.createTransaction([
  ["my-product.dot", 0],  // ProductAccountId
  {
    v1: {
      signer: null,        // host picks the signer
      callData: "0x0500...", // SCALE-encoded Call
      extensions: [
        { id: "CheckSpecVersion", extra: "0x", additionalSigned: "0x01000000" },
      ],
      txExtVersion: 0,
      context: {
        metadata: "0x...",
        tokenSymbol: "DOT",
        tokenDecimals: 10,
        bestBlockHeight: 12345678,
      },
    }
  }
]);

if (result.isOk) {
  // Submit the signed transaction
  const signedTx = result.value;
}`,
    hostExample: `container.handleCreateTransaction(
  ([productAccountId, versionedPayload], { ok, err }) => {
    if (versionedPayload.tag !== "v1") {
      return err({ NotSupported: "Only v1 supported" });
    }
    const tx = buildAndSignTransaction(
      productAccountId, versionedPayload.value
    );
    return ok(tx);
  }
);`,
  },
  {
    id: 'host_create_transaction_with_non_product_account',
    name: 'host_create_transaction_with_non_product_account',
    group: 'Signing',
    groupId: 'signing',
    pattern: 'request-response',
    description: 'Same as host_create_transaction but uses the user\'s main account instead of a product-derived account.',
    productFunction: 'hostApi.createTransactionWithNonProductAccount(payload)',
    hostHandler: 'container.handleCreateTransactionWithNonProductAccount(handler)',
    request: 'VersionedTxPayload',
    response: 'Result(Bytes, CreateTransactionErr)',
    requestDescription: 'Same VersionedTxPayload structure, without ProductAccountId',
    errorType: 'CreateTransactionErr',
    errorVariants: ['FailedToDecode', 'Rejected', 'NotSupported(str)', 'PermissionDenied', 'Unknown({ reason: str })'],
    productExample: `// Create transaction with user's main wallet account
const result = await hostApi.createTransactionWithNonProductAccount({
  v1: {
    signer: "5GrwvaEF5...",
    callData: "0x0500...",
    extensions: [],
    txExtVersion: 0,
    context: {
      metadata: "0x...",
      tokenSymbol: "DOT",
      tokenDecimals: 10,
      bestBlockHeight: 12345678,
    },
  }
});`,
    hostExample: `container.handleCreateTransactionWithNonProductAccount(
  (versionedPayload, { ok, err }) => {
    const tx = buildAndSignTransaction(
      currentUser.mainAccount, versionedPayload.value
    );
    return ok(tx);
  }
);`,
  },
  // Group 6: Chat
  {
    id: 'host_chat_create_room',
    name: 'host_chat_create_room',
    group: 'Chat',
    groupId: 'chat',
    pattern: 'request-response',
    description: 'Registers a chat room with the host.',
    productFunction: 'hostApi.chatCreateRoom(params)',
    hostHandler: 'container.handleChatCreateRoom(handler)',
    request: 'ChatRoomRequest',
    response: 'Result(ChatRoomRegistrationResult, ChatRoomRegistrationErr)',
    errorType: 'ChatRoomRegistrationErr',
    errorVariants: ['PermissionDenied', 'Unknown({ reason: str })'],
    productExample: `// Create a chat room
const result = await hostApi.chatCreateRoom({
  roomId: "general-chat",
  name: "General Discussion",
  icon: "https://example.com/chat-icon.png"
});

if (result.isOk) {
  console.log("Room status:", result.value.status);
  // "New" or "Exists"
}`,
    hostExample: `container.handleChatCreateRoom((request, { ok, err }) => {
  const existing = chatRooms.get(request.roomId);
  if (existing) {
    return ok({ status: "Exists" });
  }
  chatRooms.set(request.roomId, {
    name: request.name,
    icon: request.icon,
  });
  return ok({ status: "New" });
});`,
  },
  {
    id: 'host_chat_register_bot',
    name: 'host_chat_register_bot',
    group: 'Chat',
    groupId: 'chat',
    pattern: 'request-response',
    description: 'Registers a bot identity for chat.',
    productFunction: 'hostApi.chatRegisterBot(params)',
    hostHandler: 'container.handleChatBotRegistration(handler)',
    request: 'ChatBotRequest',
    response: 'Result(ChatBotRegistrationResult, ChatBotRegistrationErr)',
    errorType: 'ChatBotRegistrationErr',
    errorVariants: ['PermissionDenied', 'Unknown({ reason: str })'],
    productExample: `// Register a bot for automated messages
const result = await hostApi.chatRegisterBot({
  botId: "price-bot",
  name: "Price Alert Bot",
  icon: "https://example.com/bot-icon.png"
});`,
    hostExample: `container.handleChatBotRegistration((request, { ok, err }) => {
  const existing = chatBots.get(request.botId);
  if (existing) {
    return ok({ status: "Exists" });
  }
  chatBots.set(request.botId, {
    name: request.name,
    icon: request.icon,
  });
  return ok({ status: "New" });
});`,
  },
  {
    id: 'host_chat_post_message',
    name: 'host_chat_post_message',
    group: 'Chat',
    groupId: 'chat',
    pattern: 'request-response',
    description: 'Posts a message to a chat room. Supports text, rich text, actions, files, reactions, and custom messages.',
    productFunction: 'hostApi.chatPostMessage(params)',
    hostHandler: 'container.handleChatPostMessage(handler)',
    request: 'Struct { roomId: str, payload: ChatMessageContent }',
    response: 'Result(ChatPostMessageResult, ChatMessagePostingErr)',
    errorType: 'ChatMessagePostingErr',
    errorVariants: ['MessageTooLarge', 'Unknown({ reason: str })'],
    productExample: `// Post a text message
const result = await hostApi.chatPostMessage({
  roomId: "general-chat",
  payload: { Text: "Hello everyone!" }
});

// Post an action menu
const result2 = await hostApi.chatPostMessage({
  roomId: "general-chat",
  payload: {
    Actions: {
      text: "Choose an option:",
      actions: [
        { actionId: "vote-yes", title: "Vote Yes" },
        { actionId: "vote-no", title: "Vote No" },
      ],
      layout: "Grid"
    }
  }
});`,
    hostExample: `container.handleChatPostMessage(({ roomId, payload }, { ok, err }) => {
  const messageId = generateId();
  chatRooms.get(roomId)?.messages.push({
    id: messageId,
    content: payload,
    timestamp: Date.now(),
  });
  return ok({ messageId });
});`,
  },
  {
    id: 'host_chat_list_subscribe',
    name: 'host_chat_list_subscribe',
    group: 'Chat',
    groupId: 'chat',
    pattern: 'subscription',
    description: 'Subscribes to the list of chat rooms the product participates in. The host pushes the full room list whenever it changes.',
    productFunction: 'hostApi.chatListSubscribe(void, callback)',
    hostHandler: 'container.handleChatListSubscribe(handler)',
    request: 'void',
    response: 'Vector(ChatRoom)',
    productExample: `// Watch the room list
const sub = hostApi.chatListSubscribe(
  undefined,
  (rooms) => {
    console.log("Current rooms:", rooms);
    rooms.forEach(room => {
      console.log(\`  \${room.roomId} as \${room.participatingAs}\`);
    });
  }
);`,
    hostExample: `container.handleChatListSubscribe((_, send, interrupt) => {
  // Send initial room list
  send(getRoomsForProduct(productId));

  const unsub = roomStore.onChange(() => {
    send(getRoomsForProduct(productId));
  });

  return () => unsub();
});`,
  },
  {
    id: 'host_chat_action_subscribe',
    name: 'host_chat_action_subscribe',
    group: 'Chat',
    groupId: 'chat',
    pattern: 'subscription',
    description: 'Subscribes to chat actions (messages posted by peers, button clicks, commands).',
    productFunction: 'hostApi.chatActionSubscribe(void, callback)',
    hostHandler: 'container.handleChatActionSubscribe(handler)',
    request: 'void',
    response: 'ReceivedChatAction',
    productExample: `// Listen for chat events
const sub = hostApi.chatActionSubscribe(
  undefined,
  (action) => {
    const { roomId, peer, payload } = action;

    if (payload.tag === "MessagePosted") {
      handleNewMessage(roomId, peer, payload.value);
    } else if (payload.tag === "ActionTriggered") {
      handleAction(payload.value.actionId);
    } else if (payload.tag === "Command") {
      handleCommand(payload.value.command, payload.value.payload);
    }
  }
);`,
    hostExample: `container.handleChatActionSubscribe((_, send, interrupt) => {
  const unsub = chatEvents.on("action", (action) => {
    send(action);
  });
  return () => unsub();
});`,
  },
  {
    id: 'product_chat_custom_message_render_subscribe',
    name: 'product_chat_custom_message_render_subscribe',
    group: 'Chat',
    groupId: 'chat',
    pattern: 'reverse-subscription',
    description: 'Reverse-direction subscription: the host initiates, asking the product to render a custom chat message as a UI tree of CustomRendererNode components.',
    productFunction: 'createProductChatManager().onCustomMessageRenderingRequest(renderer)',
    hostHandler: 'container.renderChatCustomMessage(msg, callback)',
    request: 'Struct { messageId: str, messageType: str, payload: Bytes }',
    response: 'CustomRendererNode',
    requestDescription: 'Host sends message details for product to render',
    responseDescription: 'Recursive UI tree: Box, Column, Row, Text, Button, TextField, Spacer, Nil',
    notes: 'This is the only method where roles are reversed. The host initiates and the product responds with rendered UI.',
    productExample: `// Register a custom message renderer
const chatManager = createProductChatManager();

chatManager.onCustomMessageRenderingRequest(
  ({ messageId, messageType, payload }, render, subscribeActions) => {
    // Render a custom UI
    render({
      Column: {
        modifiers: [{ padding: [8, 12, 8, 12] }],
        props: { horizontalAlignment: "start" },
        children: [
          { Text: {
            modifiers: [],
            props: { style: "headline", color: "textPrimary" },
            children: [{ String: "Custom Poll" }]
          }},
          { Button: {
            modifiers: [],
            props: {
              text: "Vote",
              variant: "primary",
              clickAction: "vote-action"
            },
            children: []
          }}
        ]
      }
    });

    // Listen for interactions
    subscribeActions((action) => {
      if (action.actionId === "vote-action") {
        handleVote(messageId);
      }
    });
  }
);`,
    hostExample: `// Host triggers rendering of a custom message
const unsub = container.renderChatCustomMessage(
  {
    messageId: "msg-123",
    messageType: "poll",
    payload: encodedPollData,
  },
  (renderedNode) => {
    // Display the rendered CustomRendererNode tree
    updateChatUI(renderedNode);
  }
);`,
  },
  // Group 7: Statement Store
  {
    id: 'remote_statement_store_subscribe',
    name: 'remote_statement_store_subscribe',
    group: 'Statement Store',
    groupId: 'statement-store',
    pattern: 'subscription',
    description: 'Subscribes to statements matching a set of topics. The host pushes matching signed statements whenever the set changes.',
    productFunction: 'hostApi.statementStoreSubscribe(topics, callback)',
    hostHandler: 'container.handleStatementStoreSubscribe(handler)',
    request: 'Vector(Topic)',
    response: 'Vector(SignedStatement)',
    productExample: `// Subscribe to statements for specific topics
const topic = new Uint8Array(32);
topic.set([1, 2, 3]); // topic identifier

const sub = hostApi.statementStoreSubscribe(
  [topic],
  (statements) => {
    for (const stmt of statements) {
      console.log("Statement from:", stmt.proof);
      if (stmt.data) {
        processStatement(stmt.data);
      }
    }
  }
);`,
    hostExample: `container.handleStatementStoreSubscribe((topics, send, interrupt) => {
  // Send matching statements
  send(statementStore.queryByTopics(topics));

  const unsub = statementStore.onChange(topics, (statements) => {
    send(statements);
  });

  return () => unsub();
});`,
  },
  {
    id: 'remote_statement_store_create_proof',
    name: 'remote_statement_store_create_proof',
    group: 'Statement Store',
    groupId: 'statement-store',
    pattern: 'request-response',
    description: 'Creates a cryptographic proof (signature) for a statement using a product account\'s key.',
    productFunction: 'hostApi.statementStoreCreateProof(params)',
    hostHandler: 'container.handleStatementStoreCreateProof(handler)',
    request: 'Tuple(ProductAccountId, Statement)',
    response: 'Result(StatementProof, StatementProofErr)',
    requestDescription: 'ProductAccountId and a Statement to sign',
    errorType: 'StatementProofErr',
    errorVariants: ['UnableToSign', 'UnknownAccount', 'Unknown({ reason: str })'],
    productExample: `// Create a proof for a statement
const result = await hostApi.statementStoreCreateProof([
  ["my-product.dot", 0],  // ProductAccountId
  {
    proof: null,
    decryptionKey: null,
    expiry: BigInt(Date.now() + 86400000), // 24 hours
    channel: null,
    topics: [topicHash],
    data: new TextEncoder().encode("my statement"),
  }
]);

if (result.isOk) {
  const proof = result.value; // StatementProof
}`,
    hostExample: `container.handleStatementStoreCreateProof(
  ([productAccountId, statement], { ok, err }) => {
    const key = getProductKey(productAccountId);
    if (!key) {
      return err({ UnknownAccount: undefined });
    }
    const proof = key.sign(encodeStatement(statement));
    return ok({ Sr25519: { signature: proof, signer: key.publicKey } });
  }
);`,
  },
  {
    id: 'remote_statement_store_submit',
    name: 'remote_statement_store_submit',
    group: 'Statement Store',
    groupId: 'statement-store',
    pattern: 'request-response',
    description: 'Submits a signed statement to the statement store.',
    productFunction: 'hostApi.statementStoreSubmit(statement)',
    hostHandler: 'container.handleStatementStoreSubmit(handler)',
    request: 'SignedStatement',
    response: 'Result(void, GenericError)',
    requestDescription: 'See SignedStatement type for fields',
    productExample: `// Submit a signed statement
const result = await hostApi.statementStoreSubmit({
  proof: { Sr25519: { signature: sig, signer: pubKey } },
  decryptionKey: null,
  expiry: BigInt(Date.now() + 86400000),
  channel: null,
  topics: [topicHash],
  data: encodedData,
});`,
    hostExample: `container.handleStatementStoreSubmit((statement, { ok, err }) => {
  if (!verifyProof(statement.proof, statement)) {
    return err({ GenericError: { reason: "Invalid proof" } });
  }
  statementStore.insert(statement);
  return ok(undefined);
});`,
  },
  // Group 8: Preimage
  {
    id: 'remote_preimage_lookup_subscribe',
    name: 'remote_preimage_lookup_subscribe',
    group: 'Preimage',
    groupId: 'preimage',
    pattern: 'subscription',
    description: 'Subscribes to a preimage by its hash key. The host pushes the value when it becomes available.',
    productFunction: 'hostApi.preimageLookupSubscribe(key, callback)',
    hostHandler: 'container.handlePreimageLookupSubscribe(handler)',
    request: 'PreimageKey',
    response: 'Nullable(PreimageValue)',
    productExample: `// Subscribe to a preimage
const sub = hostApi.preimageLookupSubscribe(
  "0xabcdef1234...",  // hash of the preimage
  (value) => {
    if (value !== null) {
      console.log("Preimage found:", value);
    }
  }
);`,
    hostExample: `container.handlePreimageLookupSubscribe((key, send, interrupt) => {
  const existing = preimageStore.get(key);
  send(existing ?? null);

  const unsub = preimageStore.onAvailable(key, (value) => {
    send(value);
  });

  return () => unsub();
});`,
  },
  {
    id: 'remote_preimage_submit',
    name: 'remote_preimage_submit',
    group: 'Preimage',
    groupId: 'preimage',
    pattern: 'request-response',
    description: 'Submits a preimage value and receives its hash key back.',
    productFunction: 'hostApi.preimageSubmit(value)',
    hostHandler: 'container.handlePreimageSubmit(handler)',
    request: 'PreimageValue',
    response: 'Result(PreimageKey, PreimageSubmitErr)',
    errorType: 'PreimageSubmitErr',
    errorVariants: ['Unknown({ reason: str })'],
    productExample: `// Submit a preimage
const data = new TextEncoder().encode("my preimage data");
const result = await hostApi.preimageSubmit(data);

if (result.isOk) {
  console.log("Preimage key:", result.value); // hash
}`,
    hostExample: `container.handlePreimageSubmit((value, { ok, err }) => {
  const key = blake2Hash(value);
  preimageStore.set(key, value);
  return ok(key);
});`,
  },
  // Group 9: Chain Interaction
  {
    id: 'remote_chain_head_follow',
    name: 'remote_chain_head_follow',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'subscription',
    description: 'Follows the chain head, receiving events about new blocks, finalization, and operation results. Implements the chainHead_v1_follow JSON-RPC method.',
    productFunction: 'hostApi.chainHeadFollow(params, callback)',
    hostHandler: 'container.handleChainConnection(factory)',
    request: 'Struct { genesisHash: GenesisHash, withRuntime: bool }',
    response: 'ChainHeadEvent',
    responseDescription: 'Enum with 12 variants: Initialized, NewBlock, BestBlockChanged, Finalized, OperationBodyDone, OperationCallDone, OperationStorageItems, OperationStorageDone, OperationWaitingForContinue, OperationInaccessible, OperationError, Stop',
    notes: 'On the Product Side, typically used via createPapiProvider(genesisHash) from @novasamatech/product-sdk. On the host side, handled via container.handleChainConnection(factory) which manages all chain methods internally.',
    productExample: `// Follow chain head events (low-level)
const sub = hostApi.chainHeadFollow(
  { genesisHash: polkadotGenesis, withRuntime: true },
  (event) => {
    switch (event.tag) {
      case "Initialized":
        console.log("Finalized:", event.value.finalizedBlockHashes);
        break;
      case "NewBlock":
        console.log("New block:", event.value.blockHash);
        break;
      case "BestBlockChanged":
        console.log("Best:", event.value.bestBlockHash);
        break;
      case "Finalized":
        console.log("Finalized:", event.value.finalizedBlockHashes);
        break;
    }
  }
);

// Typically used via higher-level abstraction:
// const provider = createPapiProvider(polkadotGenesis);`,
    hostExample: `// Host registers a JSON-RPC provider factory
container.handleChainConnection((genesisHash) => {
  // Return a JsonRpcProvider for the requested chain
  const chain = chains.get(genesisHash);
  if (!chain) return null;

  return chain.jsonRpcProvider;
  // The chainConnectionManager handles all chain_head_*
  // methods internally via this provider
});`,
  },
  {
    id: 'remote_chain_head_header',
    name: 'remote_chain_head_header',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Retrieves a block header by hash within a follow subscription.',
    productFunction: 'hostApi.chainHeadHeader(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, followSubscriptionId: str, hash: BlockHash }',
    response: 'Result(Nullable(Hex), GenericError)',
    responseDescription: 'SCALE-encoded block header, or null',
    productExample: `const result = await hostApi.chainHeadHeader({
  genesisHash: polkadotGenesis,
  followSubscriptionId: subId,
  hash: blockHash,
});

if (result.isOk && result.value) {
  const headerBytes = result.value;
  const header = decodeHeader(headerBytes);
}`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainHead_v1_header JSON-RPC call`,
  },
  {
    id: 'remote_chain_head_body',
    name: 'remote_chain_head_body',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Retrieves a block body. Returns an operation ID; results arrive as OperationBodyDone events on the follow subscription.',
    productFunction: 'hostApi.chainHeadBody(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, followSubscriptionId: str, hash: BlockHash }',
    response: 'Result(OperationStartedResult, GenericError)',
    responseDescription: 'Started { operationId: OperationId } or LimitReached',
    productExample: `const result = await hostApi.chainHeadBody({
  genesisHash: polkadotGenesis,
  followSubscriptionId: subId,
  hash: blockHash,
});

if (result.isOk && result.value.tag === "Started") {
  const opId = result.value.value.operationId;
  // Wait for OperationBodyDone event on follow subscription
}`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainHead_v1_body JSON-RPC call`,
  },
  {
    id: 'remote_chain_head_storage',
    name: 'remote_chain_head_storage',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Queries chain storage. Returns an operation ID; results arrive as OperationStorageItems/OperationStorageDone events.',
    productFunction: 'hostApi.chainHeadStorage(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, followSubscriptionId: str, hash: BlockHash, items: Vector(StorageQueryItem), childTrie: Nullable(Hex) }',
    response: 'Result(OperationStartedResult, GenericError)',
    productExample: `const result = await hostApi.chainHeadStorage({
  genesisHash: polkadotGenesis,
  followSubscriptionId: subId,
  hash: blockHash,
  items: [
    { key: "0x26aa394eea5630e07c48ae0c9558cef7", type: "Value" }
  ],
  childTrie: null,
});`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainHead_v1_storage JSON-RPC call`,
  },
  {
    id: 'remote_chain_head_call',
    name: 'remote_chain_head_call',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Executes a runtime API call. Returns an operation ID; result arrives as OperationCallDone event.',
    productFunction: 'hostApi.chainHeadCall(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, followSubscriptionId: str, hash: BlockHash, function: str, callParameters: Hex }',
    response: 'Result(OperationStartedResult, GenericError)',
    productExample: `const result = await hostApi.chainHeadCall({
  genesisHash: polkadotGenesis,
  followSubscriptionId: subId,
  hash: blockHash,
  function: "Metadata_metadata",
  callParameters: "0x",
});`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainHead_v1_call JSON-RPC call`,
  },
  {
    id: 'remote_chain_head_unpin',
    name: 'remote_chain_head_unpin',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Unpins block hashes, allowing the node to discard them.',
    productFunction: 'hostApi.chainHeadUnpin(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, followSubscriptionId: str, hashes: Vector(BlockHash) }',
    response: 'Result(void, GenericError)',
    productExample: `await hostApi.chainHeadUnpin({
  genesisHash: polkadotGenesis,
  followSubscriptionId: subId,
  hashes: [oldBlockHash1, oldBlockHash2],
});`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainHead_v1_unpin JSON-RPC call`,
  },
  {
    id: 'remote_chain_head_continue',
    name: 'remote_chain_head_continue',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Continues a paused operation (when OperationWaitingForContinue is received).',
    productFunction: 'hostApi.chainHeadContinue(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, followSubscriptionId: str, operationId: OperationId }',
    response: 'Result(void, GenericError)',
    productExample: `// When OperationWaitingForContinue is received:
await hostApi.chainHeadContinue({
  genesisHash: polkadotGenesis,
  followSubscriptionId: subId,
  operationId: opId,
});`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainHead_v1_continue JSON-RPC call`,
  },
  {
    id: 'remote_chain_head_stop_operation',
    name: 'remote_chain_head_stop_operation',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Stops an in-progress operation.',
    productFunction: 'hostApi.chainHeadStopOperation(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, followSubscriptionId: str, operationId: OperationId }',
    response: 'Result(void, GenericError)',
    productExample: `await hostApi.chainHeadStopOperation({
  genesisHash: polkadotGenesis,
  followSubscriptionId: subId,
  operationId: opId,
});`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainHead_v1_stopOperation JSON-RPC call`,
  },
  {
    id: 'remote_chain_spec_genesis_hash',
    name: 'remote_chain_spec_genesis_hash',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Gets the genesis hash for a chain.',
    productFunction: 'hostApi.chainSpecGenesisHash(genesisHash)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'GenesisHash',
    response: 'Result(Hex, GenericError)',
    productExample: `const result = await hostApi.chainSpecGenesisHash(
  polkadotGenesis
);`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainSpec_v1_genesisHash JSON-RPC call`,
  },
  {
    id: 'remote_chain_spec_chain_name',
    name: 'remote_chain_spec_chain_name',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Gets the chain name.',
    productFunction: 'hostApi.chainSpecChainName(genesisHash)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'GenesisHash',
    response: 'Result(str, GenericError)',
    productExample: `const result = await hostApi.chainSpecChainName(
  polkadotGenesis
);
if (result.isOk) {
  console.log("Chain:", result.value); // "Polkadot"
}`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainSpec_v1_chainName JSON-RPC call`,
  },
  {
    id: 'remote_chain_spec_properties',
    name: 'remote_chain_spec_properties',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Gets the chain properties as a JSON-encoded string.',
    productFunction: 'hostApi.chainSpecProperties(genesisHash)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'GenesisHash',
    response: 'Result(str, GenericError)',
    responseDescription: 'JSON-encoded chain properties',
    productExample: `const result = await hostApi.chainSpecProperties(
  polkadotGenesis
);
if (result.isOk) {
  const props = JSON.parse(result.value);
  console.log("Token:", props.tokenSymbol); // "DOT"
}`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to chainSpec_v1_properties JSON-RPC call`,
  },
  {
    id: 'remote_chain_transaction_broadcast',
    name: 'remote_chain_transaction_broadcast',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Broadcasts a signed transaction to the network.',
    productFunction: 'hostApi.chainTransactionBroadcast(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, transaction: Hex }',
    response: 'Result(Nullable(str), GenericError)',
    responseDescription: 'Operation ID if accepted, null if rejected',
    productExample: `const result = await hostApi.chainTransactionBroadcast({
  genesisHash: polkadotGenesis,
  transaction: signedTxHex,
});

if (result.isOk && result.value) {
  console.log("Broadcasting, op:", result.value);
}`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to transaction_v1_broadcast JSON-RPC call`,
  },
  {
    id: 'remote_chain_transaction_stop',
    name: 'remote_chain_transaction_stop',
    group: 'Chain Interaction',
    groupId: 'chain-interaction',
    pattern: 'request-response',
    description: 'Stops broadcasting a transaction.',
    productFunction: 'hostApi.chainTransactionStop(params)',
    hostHandler: 'Managed by chainConnectionManager',
    request: 'Struct { genesisHash: GenesisHash, operationId: OperationId }',
    response: 'Result(void, GenericError)',
    productExample: `await hostApi.chainTransactionStop({
  genesisHash: polkadotGenesis,
  operationId: broadcastOpId,
});`,
    hostExample: `// Handled automatically by chainConnectionManager
// translates to transaction_v1_stop JSON-RPC call`,
  },
];

export const dataTypes: DataType[] = [
  // Primitive Types
  { id: 'str', name: 'str', category: 'Primitives', definition: 'length-prefixed UTF-8', description: 'String value, SCALE-encoded as length-prefixed UTF-8 bytes.' },
  { id: 'bool', name: 'bool', category: 'Primitives', definition: 'single byte', description: 'Boolean value encoded as a single byte (0x00 = false, 0x01 = true).' },
  { id: 'u8', name: 'u8', category: 'Primitives', definition: '1 byte unsigned', description: '8-bit unsigned integer.' },
  { id: 'u32', name: 'u32', category: 'Primitives', definition: '4 bytes LE unsigned', description: '32-bit unsigned integer, little-endian encoded.' },
  { id: 'u64', name: 'u64', category: 'Primitives', definition: '8 bytes LE unsigned', description: '64-bit unsigned integer, little-endian encoded.' },
  { id: 'compact', name: 'compact', category: 'Primitives', definition: 'SCALE compact integer', description: 'Variable-length unsigned integer using SCALE compact encoding.' },
  { id: 'Hex', name: 'Hex / Hex()', category: 'Primitives', definition: 'length-prefixed bytes', description: 'Arbitrary hex-encoded bytes, SCALE length-prefixed.' },
  { id: 'Bytes', name: 'Bytes()', category: 'Primitives', definition: 'length-prefixed bytes', description: 'Arbitrary binary data, SCALE length-prefixed.' },
  { id: 'BytesN', name: 'Bytes(N)', category: 'Primitives', definition: 'fixed N bytes', description: 'Fixed-length binary data of exactly N bytes.' },
  { id: '_void', name: '_void', category: 'Primitives', definition: 'zero bytes', description: 'Unit type / no data. Takes zero bytes on the wire.' },
  // Composite Combinators
  { id: 'Option', name: 'Option(T)', category: 'Combinators', definition: 'None (0x00) or Some(T) (0x01 + encoded T)', description: 'Optional value. Encoded as 0x00 for None, or 0x01 followed by the encoded inner value.' },
  { id: 'Nullable', name: 'Nullable(T)', category: 'Combinators', definition: 'Null or T', description: 'Similar to Option but with different encoding semantics for null values.' },
  { id: 'Vector', name: 'Vector(T)', category: 'Combinators', definition: 'Length-prefixed array of T', description: 'A variable-length array. Encoded as a compact length prefix followed by each element.' },
  { id: 'Tuple', name: 'Tuple(A, B, ...)', category: 'Combinators', definition: 'Concatenated encodings of A, B, ...', description: 'Fixed-size collection of values of potentially different types, encoded by concatenation.' },
  { id: 'Struct', name: 'Struct({ k: T, ... })', category: 'Combinators', definition: 'Concatenated encodings of fields in definition order', description: 'Named fields encoded in declaration order by concatenation.' },
  { id: 'Enum', name: 'Enum({ V1: T1, V2: T2, ... })', category: 'Combinators', definition: 'Tag byte + variant encoding', description: 'Tagged union. A single tag byte selects the variant, followed by that variant\'s encoding.' },
  { id: 'Status', name: 'Status(s1, s2, ...)', category: 'Combinators', definition: 'Enum where each variant carries _void', description: 'Enumeration of named states, each carrying no data (all variants are _void).' },
  { id: 'Result', name: 'Result(Ok, Err)', category: 'Combinators', definition: '0x00 + Ok encoding, or 0x01 + Err encoding', description: 'Success/failure wrapper. 0x00 prefix for Ok, 0x01 prefix for Err.' },
  { id: 'ErrEnum', name: 'ErrEnum(name, variants)', category: 'Combinators', definition: 'Error enum with descriptive variant names', description: 'Specialized enum used for error types with human-readable variant names.' },
  // Common Types
  { id: 'GenesisHash', name: 'GenesisHash', category: 'Common', source: 'commonCodecs.ts', definition: 'Hex()', description: 'Blockchain genesis hash, used to identify a specific chain.' },
  { id: 'GenericErr', name: 'GenericErr', category: 'Common', source: 'commonCodecs.ts', definition: 'Struct({ reason: str })', description: 'Generic error payload carrying a human-readable reason string.' },
  { id: 'GenericError', name: 'GenericError', category: 'Common', source: 'commonCodecs.ts', definition: "ErrEnum { GenericError(GenericErr) }", description: 'Single-variant error enum wrapping GenericErr. Used by many methods as a catch-all error type.' },
  // Account Types
  {
    id: 'AccountId', name: 'AccountId', category: 'Account', source: 'accounts.ts', definition: 'Bytes(32)',
    description: '32-byte account identifier (typically an SS58 public key).',
  },
  {
    id: 'PublicKey', name: 'PublicKey', category: 'Account', source: 'accounts.ts', definition: 'Bytes()',
    description: 'Variable-length public key.',
  },
  {
    id: 'DotNsIdentifier', name: 'DotNsIdentifier', category: 'Account', source: 'accounts.ts', definition: 'str',
    description: 'A dotNS domain name identifier (e.g., "my-product.dot").',
  },
  {
    id: 'DerivationIndex', name: 'DerivationIndex', category: 'Account', source: 'accounts.ts', definition: 'u32',
    description: 'Key derivation index for generating product-specific accounts.',
  },
  {
    id: 'ProductAccountId', name: 'ProductAccountId', category: 'Account', source: 'accounts.ts',
    definition: 'Tuple(DotNsIdentifier, DerivationIndex)',
    description: 'Identifies a product-specific account by combining a dotNS domain name with a derivation index.',
  },
  {
    id: 'Account', name: 'Account', category: 'Account', source: 'accounts.ts',
    definition: 'Struct({ publicKey: PublicKey, name: Option(str) })',
    description: 'An account with its public key and optional display name.',
    fields: [
      { name: 'publicKey', type: 'PublicKey', description: 'The account public key (variable-length Bytes)' },
      { name: 'name', type: 'Option(str)', description: 'Optional human-readable display name' },
    ],
  },
  {
    id: 'ContextualAlias', name: 'ContextualAlias', category: 'Account', source: 'accounts.ts',
    definition: 'Struct({ context: Bytes(32), alias: Bytes() })',
    description: 'A privacy-preserving alias derived via ring VRF, bound to a specific context.',
    fields: [
      { name: 'context', type: 'Bytes(32)', description: '32-byte context identifier' },
      { name: 'alias', type: 'Bytes()', description: 'Ring VRF alias (variable length)' },
    ],
  },
  {
    id: 'RingLocationHint', name: 'RingLocationHint', category: 'Account', source: 'accounts.ts',
    definition: 'Struct({ palletInstance: Option(u32) })',
    description: 'Hints for locating a ring on-chain.',
    fields: [{ name: 'palletInstance', type: 'Option(u32)', description: 'Optional pallet instance index' }],
  },
  {
    id: 'RingLocation', name: 'RingLocation', category: 'Account', source: 'accounts.ts',
    definition: 'Struct({ genesisHash: GenesisHash, ringRootHash: Hex(), hints: Option(RingLocationHint) })',
    description: 'Locates a specific ring on a specific chain for ring VRF operations.',
    fields: [
      { name: 'genesisHash', type: 'GenesisHash', description: 'Chain genesis hash' },
      { name: 'ringRootHash', type: 'Hex()', description: 'Root hash of the ring' },
      { name: 'hints', type: 'Option(RingLocationHint)', description: 'Optional location hints' },
    ],
  },
  { id: 'RingVrfProof', name: 'RingVrfProof', category: 'Account', source: 'accounts.ts', definition: 'Bytes()', description: 'Variable-length ring VRF proof bytes.' },
  // Account Error Types
  {
    id: 'RequestCredentialsErr', name: 'RequestCredentialsErr', category: 'Account', source: 'accounts.ts',
    definition: 'ErrEnum { NotConnected, Rejected, DomainNotValid, Unknown({ reason: str }) }',
    description: 'Error returned when credential/account requests fail.',
    variants: [
      { name: 'NotConnected', type: '_void', description: 'User is not logged in' },
      { name: 'Rejected', type: '_void', description: 'User or host rejected the request' },
      { name: 'DomainNotValid', type: '_void', description: 'Domain identifier is invalid' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all error with reason' },
    ],
  },
  {
    id: 'CreateProofErr', name: 'CreateProofErr', category: 'Account', source: 'accounts.ts',
    definition: 'ErrEnum { RingNotFound, Rejected, Unknown({ reason: str }) }',
    description: 'Error returned when ring VRF proof creation fails.',
    variants: [
      { name: 'RingNotFound', type: '_void', description: 'Ring not available at the specified location' },
      { name: 'Rejected', type: '_void', description: 'User or host rejected' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  {
    id: 'AccountConnectionStatus', name: 'AccountConnectionStatus', category: 'Account', source: 'accounts.ts',
    definition: "Status('disconnected', 'connected')",
    description: 'Status enum representing the user\'s authentication state.',
  },
  // Signing Types
  {
    id: 'SigningPayload', name: 'SigningPayload', category: 'Signing', source: 'sign.ts',
    definition: 'Struct({ address, blockHash, blockNumber, era, genesisHash, method, nonce, specVersion, tip, transactionVersion, signedExtensions, version, assetId?, metadataHash?, mode?, withSignedTransaction? })',
    description: 'Full Substrate extrinsic signing payload with all fields needed for signature generation.',
    fields: [
      { name: 'address', type: 'str', description: 'Signer address (SS58 or hex)' },
      { name: 'blockHash', type: 'Hex()', description: 'Reference block hash' },
      { name: 'blockNumber', type: 'Hex()', description: 'Reference block number' },
      { name: 'era', type: 'Hex()', description: 'Mortality era encoding' },
      { name: 'genesisHash', type: 'GenesisHash', description: 'Chain genesis hash' },
      { name: 'method', type: 'Hex()', description: 'SCALE-encoded call data' },
      { name: 'nonce', type: 'Hex()', description: 'Account nonce' },
      { name: 'specVersion', type: 'Hex()', description: 'Runtime spec version' },
      { name: 'tip', type: 'Hex()', description: 'Transaction tip' },
      { name: 'transactionVersion', type: 'Hex()', description: 'Transaction format version' },
      { name: 'signedExtensions', type: 'Vector(str)', description: 'Extension identifiers' },
      { name: 'version', type: 'u32', description: 'Extrinsic version' },
      { name: 'assetId', type: 'Option(Hex())', description: 'For multi-asset tips' },
      { name: 'metadataHash', type: 'Option(Hex())', description: 'CheckMetadataHash extension' },
      { name: 'mode', type: 'Option(u32)', description: 'Metadata mode' },
      { name: 'withSignedTransaction', type: 'Option(bool)', description: 'Request signed tx back' },
    ],
  },
  {
    id: 'RawPayload', name: 'RawPayload', category: 'Signing', source: 'sign.ts',
    definition: 'Enum({ Bytes: Bytes(), Payload: str })',
    description: 'Raw data to sign — either binary bytes or a string message.',
    variants: [
      { name: 'Bytes', type: 'Bytes()', description: 'Raw binary data to sign' },
      { name: 'Payload', type: 'str', description: 'String message to sign' },
    ],
  },
  {
    id: 'SigningRawPayload', name: 'SigningRawPayload', category: 'Signing', source: 'sign.ts',
    definition: 'Struct({ address: str, data: RawPayload })',
    description: 'A raw signing request pairing an address with raw data.',
    fields: [
      { name: 'address', type: 'str', description: 'Signer address' },
      { name: 'data', type: 'RawPayload', description: 'The data to sign' },
    ],
  },
  {
    id: 'SigningResult', name: 'SigningResult', category: 'Signing', source: 'sign.ts',
    definition: 'Struct({ signature: Hex(), signedTransaction: Option(Hex()) })',
    description: 'Result of a signing operation.',
    fields: [
      { name: 'signature', type: 'Hex()', description: 'The cryptographic signature' },
      { name: 'signedTransaction', type: 'Option(Hex())', description: 'Full signed transaction, if requested' },
    ],
  },
  {
    id: 'SigningErr', name: 'SigningErr', category: 'Signing', source: 'sign.ts',
    definition: 'ErrEnum { FailedToDecode, Rejected, PermissionDenied, Unknown({ reason: str }) }',
    description: 'Signing operation error.',
    variants: [
      { name: 'FailedToDecode', type: '_void', description: 'Payload could not be deserialized' },
      { name: 'Rejected', type: '_void', description: 'User rejected signing' },
      { name: 'PermissionDenied', type: '_void', description: 'Not authenticated' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  // Transaction Creation Types
  {
    id: 'TxPayloadExtensionV1', name: 'TxPayloadExtensionV1', category: 'Transaction', source: 'createTransaction.ts',
    definition: 'Struct({ id: str, extra: Hex(), additionalSigned: Hex() })',
    description: 'A signed extension for a transaction payload.',
    fields: [
      { name: 'id', type: 'str', description: 'Extension name (e.g., "CheckSpecVersion")' },
      { name: 'extra', type: 'Hex()', description: 'SCALE-encoded extra data (in extrinsic body)' },
      { name: 'additionalSigned', type: 'Hex()', description: 'SCALE-encoded implicit data (signed, not in body)' },
    ],
  },
  {
    id: 'TxPayloadContextV1', name: 'TxPayloadContextV1', category: 'Transaction', source: 'createTransaction.ts',
    definition: 'Struct({ metadata: Hex(), tokenSymbol: str, tokenDecimals: u32, bestBlockHeight: u32 })',
    description: 'Context information for transaction construction.',
    fields: [
      { name: 'metadata', type: 'Hex()', description: 'RuntimeMetadataPrefixed blob (SCALE)' },
      { name: 'tokenSymbol', type: 'str', description: 'Native token symbol' },
      { name: 'tokenDecimals', type: 'u32', description: 'Native token decimals' },
      { name: 'bestBlockHeight', type: 'u32', description: 'Highest known block number' },
    ],
  },
  {
    id: 'TxPayloadV1', name: 'TxPayloadV1', category: 'Transaction', source: 'createTransaction.ts',
    definition: 'Struct({ signer: Nullable(str), callData: Hex(), extensions: Vector(TxPayloadExtensionV1), txExtVersion: u8, context: TxPayloadContextV1 })',
    description: 'Version 1 transaction payload with all data needed to construct a signed extrinsic.',
    fields: [
      { name: 'signer', type: 'Nullable(str)', description: 'Signer hint (address/name), null = host picks' },
      { name: 'callData', type: 'Hex()', description: 'SCALE-encoded Call data' },
      { name: 'extensions', type: 'Vector(TxPayloadExtensionV1)', description: 'Signed extensions' },
      { name: 'txExtVersion', type: 'u8', description: '0 for Extrinsic V4, any for V5' },
      { name: 'context', type: 'TxPayloadContextV1', description: 'Transaction context' },
    ],
  },
  {
    id: 'VersionedTxPayload', name: 'VersionedTxPayload', category: 'Transaction', source: 'createTransaction.ts',
    definition: 'Enum({ v1: TxPayloadV1 })',
    description: 'Versioned transaction payload envelope.',
    variants: [{ name: 'v1', type: 'TxPayloadV1', description: 'Version 1 payload' }],
  },
  {
    id: 'CreateTransactionErr', name: 'CreateTransactionErr', category: 'Transaction', source: 'createTransaction.ts',
    definition: 'ErrEnum { FailedToDecode, Rejected, NotSupported(str), PermissionDenied, Unknown({ reason: str }) }',
    description: 'Transaction creation error.',
    variants: [
      { name: 'FailedToDecode', type: '_void', description: 'Payload could not be deserialized' },
      { name: 'Rejected', type: '_void', description: 'User rejected' },
      { name: 'NotSupported', type: 'str', description: 'Unsupported payload version or extension' },
      { name: 'PermissionDenied', type: '_void', description: 'Not authenticated' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  // Local Storage Types
  { id: 'StorageKey', name: 'StorageKey', category: 'Storage', source: 'localStorage.ts', definition: 'str', description: 'Key name for local storage operations.' },
  { id: 'StorageValue', name: 'StorageValue', category: 'Storage', source: 'localStorage.ts', definition: 'Bytes()', description: 'Binary value stored in local storage.' },
  {
    id: 'StorageErr', name: 'StorageErr', category: 'Storage', source: 'localStorage.ts',
    definition: 'ErrEnum { Full, Unknown({ reason: str }) }',
    description: 'Local storage operation error.',
    variants: [
      { name: 'Full', type: '_void', description: 'Storage quota exceeded' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  // Navigation Types
  {
    id: 'NavigateToErr', name: 'NavigateToErr', category: 'Navigation', source: 'navigation.ts',
    definition: 'ErrEnum { PermissionDenied, Unknown({ reason: str }) }',
    description: 'Navigation error.',
    variants: [
      { name: 'PermissionDenied', type: '_void', description: 'Navigation not allowed' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  // Notification Types
  {
    id: 'PushNotification', name: 'PushNotification', category: 'Notification', source: 'notification.ts',
    definition: 'Struct({ text: str, deeplink: Option(str) })',
    description: 'Push notification payload.',
    fields: [
      { name: 'text', type: 'str', description: 'Notification text' },
      { name: 'deeplink', type: 'Option(str)', description: 'Optional URL to open on tap' },
    ],
  },
  // Permission Types
  {
    id: 'DevicePermissionRequest', name: 'DevicePermissionRequest', category: 'Permission', source: 'devicePermission.ts',
    definition: "Status('Camera', 'Microphone', 'Bluetooth', 'Location')",
    description: 'Device capability to request access to.',
  },
  {
    id: 'RemotePermissionRequest', name: 'RemotePermissionRequest', category: 'Permission', source: 'remotePermission.ts',
    definition: 'Enum({ ExternalRequest: str, TransactionSubmit: _void })',
    description: 'Remote operation permission request.',
    variants: [
      { name: 'ExternalRequest', type: 'str', description: 'URL the product wants to fetch' },
      { name: 'TransactionSubmit', type: '_void', description: 'product wants to submit a transaction' },
    ],
  },
  // Feature Types
  {
    id: 'Feature', name: 'Feature', category: 'Feature', source: 'feature.ts',
    definition: 'Enum({ Chain: GenesisHash })',
    description: 'Feature to check for host support.',
    variants: [{ name: 'Chain', type: 'GenesisHash', description: 'Is this blockchain supported?' }],
  },
  // Chat Types
  {
    id: 'ChatRoomRequest', name: 'ChatRoomRequest', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ roomId: str, name: str, icon: str })',
    description: 'Request to create a chat room.',
    fields: [
      { name: 'roomId', type: 'str', description: 'Unique room identifier' },
      { name: 'name', type: 'str', description: 'Room display name' },
      { name: 'icon', type: 'str', description: 'URL or base64 image' },
    ],
  },
  { id: 'ChatRoomRegistrationStatus', name: 'ChatRoomRegistrationStatus', category: 'Chat', source: 'chat.ts', definition: "Status('New', 'Exists')", description: 'Whether the room was newly created or already existed.' },
  {
    id: 'ChatRoomRegistrationResult', name: 'ChatRoomRegistrationResult', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ status: ChatRoomRegistrationStatus })',
    description: 'Result of a room registration.',
    fields: [{ name: 'status', type: 'ChatRoomRegistrationStatus', description: '"New" or "Exists"' }],
  },
  {
    id: 'ChatBotRequest', name: 'ChatBotRequest', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ botId: str, name: str, icon: str })',
    description: 'Request to register a chat bot.',
    fields: [
      { name: 'botId', type: 'str', description: 'Unique bot identifier' },
      { name: 'name', type: 'str', description: 'Bot display name' },
      { name: 'icon', type: 'str', description: 'URL or base64 image' },
    ],
  },
  { id: 'ChatBotRegistrationStatus', name: 'ChatBotRegistrationStatus', category: 'Chat', source: 'chat.ts', definition: "Status('New', 'Exists')", description: 'Whether the bot was newly registered or already existed.' },
  {
    id: 'ChatBotRegistrationResult', name: 'ChatBotRegistrationResult', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ status: ChatBotRegistrationStatus })',
    description: 'Result of a bot registration.',
    fields: [{ name: 'status', type: 'ChatBotRegistrationStatus', description: '"New" or "Exists"' }],
  },
  { id: 'ChatRoomParticipation', name: 'ChatRoomParticipation', category: 'Chat', source: 'chat.ts', definition: "Status('RoomHost', 'Bot')", description: 'How the product participates in a chat room.' },
  {
    id: 'ChatRoom', name: 'ChatRoom', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ roomId: str, participatingAs: ChatRoomParticipation })',
    description: 'A chat room the product participates in.',
    fields: [
      { name: 'roomId', type: 'str', description: 'Room identifier' },
      { name: 'participatingAs', type: 'ChatRoomParticipation', description: '"RoomHost" or "Bot"' },
    ],
  },
  {
    id: 'ChatAction', name: 'ChatAction', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ actionId: str, title: str })',
    description: 'A clickable action button in a chat message.',
    fields: [
      { name: 'actionId', type: 'str', description: 'Action identifier' },
      { name: 'title', type: 'str', description: 'Button label' },
    ],
  },
  { id: 'ChatActionLayout', name: 'ChatActionLayout', category: 'Chat', source: 'chat.ts', definition: "Status('Column', 'Grid')", description: 'Layout for action buttons.' },
  {
    id: 'ChatActions', name: 'ChatActions', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ text: Option(str), actions: Vector(ChatAction), layout: ChatActionLayout })',
    description: 'A set of action buttons with optional text.',
    fields: [
      { name: 'text', type: 'Option(str)', description: 'Optional message text' },
      { name: 'actions', type: 'Vector(ChatAction)', description: 'List of action buttons' },
      { name: 'layout', type: 'ChatActionLayout', description: '"Column" or "Grid" layout' },
    ],
  },
  {
    id: 'ChatMedia', name: 'ChatMedia', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ url: str })',
    description: 'A media attachment.',
    fields: [{ name: 'url', type: 'str', description: 'Media URL' }],
  },
  {
    id: 'ChatRichText', name: 'ChatRichText', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ text: Option(str), media: Vector(ChatMedia) })',
    description: 'Rich text message with optional media.',
    fields: [
      { name: 'text', type: 'Option(str)', description: 'Optional text content' },
      { name: 'media', type: 'Vector(ChatMedia)', description: 'Attached media items' },
    ],
  },
  {
    id: 'ChatFile', name: 'ChatFile', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ url: str, fileName: str, mimeType: str, sizeBytes: u64, text: Option(str) })',
    description: 'A file attachment in a chat message.',
    fields: [
      { name: 'url', type: 'str', description: 'File download URL' },
      { name: 'fileName', type: 'str', description: 'File name' },
      { name: 'mimeType', type: 'str', description: 'MIME type' },
      { name: 'sizeBytes', type: 'u64', description: 'File size in bytes' },
      { name: 'text', type: 'Option(str)', description: 'Optional caption text' },
    ],
  },
  {
    id: 'ChatReaction', name: 'ChatReaction', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ messageId: str, emoji: str })',
    description: 'A reaction to a chat message.',
    fields: [
      { name: 'messageId', type: 'str', description: 'Message being reacted to' },
      { name: 'emoji', type: 'str', description: 'Emoji reaction' },
    ],
  },
  {
    id: 'ChatCustomMessage', name: 'ChatCustomMessage', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ messageType: str, payload: Bytes() })',
    description: 'A custom message with application-defined type and binary payload.',
    fields: [
      { name: 'messageType', type: 'str', description: 'Application-defined type key' },
      { name: 'payload', type: 'Bytes()', description: 'Binary payload' },
    ],
  },
  {
    id: 'ChatMessageContent', name: 'ChatMessageContent', category: 'Chat', source: 'chat.ts',
    definition: 'Enum({ Text: str, RichText: ChatRichText, Actions: ChatActions, File: ChatFile, Reaction: ChatReaction, ReactionRemoved: ChatReaction, Custom: ChatCustomMessage })',
    description: 'Content of a chat message — one of several types.',
    variants: [
      { name: 'Text', type: 'str', description: 'Plain text message' },
      { name: 'RichText', type: 'ChatRichText', description: 'Rich text with media' },
      { name: 'Actions', type: 'ChatActions', description: 'Action button set' },
      { name: 'File', type: 'ChatFile', description: 'File attachment' },
      { name: 'Reaction', type: 'ChatReaction', description: 'Emoji reaction' },
      { name: 'ReactionRemoved', type: 'ChatReaction', description: 'Reaction removal' },
      { name: 'Custom', type: 'ChatCustomMessage', description: 'Custom message' },
    ],
  },
  {
    id: 'ChatPostMessageResult', name: 'ChatPostMessageResult', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ messageId: str })',
    description: 'Result of posting a message.',
    fields: [{ name: 'messageId', type: 'str', description: 'Assigned message ID' }],
  },
  {
    id: 'ActionTrigger', name: 'ActionTrigger', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ messageId: str, actionId: str, payload: Option(Bytes()) })',
    description: 'Payload when a user clicks an action button.',
    fields: [
      { name: 'messageId', type: 'str', description: 'Message containing the action' },
      { name: 'actionId', type: 'str', description: 'Which action was triggered' },
      { name: 'payload', type: 'Option(Bytes())', description: 'Optional additional data' },
    ],
  },
  {
    id: 'ChatCommand', name: 'ChatCommand', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ command: str, payload: str })',
    description: 'A slash command from a chat user.',
    fields: [
      { name: 'command', type: 'str', description: 'Command name' },
      { name: 'payload', type: 'str', description: 'Command arguments' },
    ],
  },
  {
    id: 'ChatActionPayload', name: 'ChatActionPayload', category: 'Chat', source: 'chat.ts',
    definition: 'Enum({ MessagePosted: ChatMessageContent, ActionTriggered: ActionTrigger, Command: ChatCommand })',
    description: 'Payload of a received chat action.',
    variants: [
      { name: 'MessagePosted', type: 'ChatMessageContent', description: 'A peer posted a message' },
      { name: 'ActionTriggered', type: 'ActionTrigger', description: 'A user triggered an action button' },
      { name: 'Command', type: 'ChatCommand', description: 'A user issued a command' },
    ],
  },
  {
    id: 'ReceivedChatAction', name: 'ReceivedChatAction', category: 'Chat', source: 'chat.ts',
    definition: 'Struct({ roomId: str, peer: str, payload: ChatActionPayload })',
    description: 'A chat action received from the host.',
    fields: [
      { name: 'roomId', type: 'str', description: 'Room where the action occurred' },
      { name: 'peer', type: 'str', description: 'Peer who initiated the action' },
      { name: 'payload', type: 'ChatActionPayload', description: 'The action payload' },
    ],
  },
  // Chat Error Types
  {
    id: 'ChatRoomRegistrationErr', name: 'ChatRoomRegistrationErr', category: 'Chat', source: 'chat.ts',
    definition: 'ErrEnum { PermissionDenied, Unknown({ reason: str }) }',
    description: 'Chat room registration error.',
    variants: [
      { name: 'PermissionDenied', type: '_void', description: 'Not allowed' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  {
    id: 'ChatBotRegistrationErr', name: 'ChatBotRegistrationErr', category: 'Chat', source: 'chat.ts',
    definition: 'ErrEnum { PermissionDenied, Unknown({ reason: str }) }',
    description: 'Chat bot registration error.',
    variants: [
      { name: 'PermissionDenied', type: '_void', description: 'Not allowed' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  {
    id: 'ChatMessagePostingErr', name: 'ChatMessagePostingErr', category: 'Chat', source: 'chat.ts',
    definition: 'ErrEnum { MessageTooLarge, Unknown({ reason: str }) }',
    description: 'Chat message posting error.',
    variants: [
      { name: 'MessageTooLarge', type: '_void', description: 'Message exceeded size limit' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  // Custom Renderer Types
  { id: 'Size', name: 'Size', category: 'Custom Renderer', source: 'customRenderer.ts', definition: 'compact', description: 'Variable-length unsigned integer used for dimensions.' },
  {
    id: 'Dimensions', name: 'Dimensions', category: 'Custom Renderer', source: 'customRenderer.ts',
    definition: 'Tuple(Size, Size, Option(Size), Option(Size))',
    description: 'CSS-like dimensions: (top, end, bottom?, start?). Bottom defaults to top, start defaults to end.',
  },
  { id: 'TypographyStyle', name: 'TypographyStyle', category: 'Custom Renderer', source: 'customRenderer.ts', definition: "Status('titleXL', 'headline', 'bodyM', 'bodyS', 'caption')", description: 'Text typography presets.' },
  { id: 'ButtonVariant', name: 'ButtonVariant', category: 'Custom Renderer', source: 'customRenderer.ts', definition: "Status('primary', 'secondary', 'text')", description: 'Button style variants.' },
  { id: 'ColorToken', name: 'ColorToken', category: 'Custom Renderer', source: 'customRenderer.ts', definition: "Status('textPrimary', 'textSecondary', 'textTertiary', 'backgroundPrimary', 'backgroundSecondary', 'backgroundTertiary', 'success', 'error', 'warning')", description: 'Semantic color tokens for theming.' },
  { id: 'ContentAlignment', name: 'ContentAlignment', category: 'Custom Renderer', source: 'customRenderer.ts', definition: "Status('topStart', 'topCenter', 'topEnd', 'centerStart', 'center', 'centerEnd', 'bottomStart', 'bottomCenter', 'bottomEnd')", description: '2D content alignment.' },
  { id: 'HorizontalAlignment', name: 'HorizontalAlignment', category: 'Custom Renderer', source: 'customRenderer.ts', definition: "Status('start', 'center', 'end')", description: 'Horizontal alignment options.' },
  { id: 'VerticalAlignment', name: 'VerticalAlignment', category: 'Custom Renderer', source: 'customRenderer.ts', definition: "Status('top', 'center', 'bottom')", description: 'Vertical alignment options.' },
  { id: 'Arrangement', name: 'Arrangement', category: 'Custom Renderer', source: 'customRenderer.ts', definition: "Status('start', 'end', 'center', 'spaceBetween', 'spaceAround', 'spaceEvenly')", description: 'Layout arrangement (like CSS flexbox justify-content).' },
  {
    id: 'Shape', name: 'Shape', category: 'Custom Renderer', source: 'customRenderer.ts',
    definition: 'Enum({ Rounded: Size, Circle: _void })',
    description: 'Shape for borders and backgrounds.',
    variants: [
      { name: 'Rounded', type: 'Size', description: 'Border radius value' },
      { name: 'Circle', type: '_void', description: 'Circular shape' },
    ],
  },
  {
    id: 'BorderStyle', name: 'BorderStyle', category: 'Custom Renderer', source: 'customRenderer.ts',
    definition: 'Struct({ width: Size, color: ColorToken, shape: Option(Shape) })',
    description: 'Border styling.',
    fields: [
      { name: 'width', type: 'Size', description: 'Border width' },
      { name: 'color', type: 'ColorToken', description: 'Border color' },
      { name: 'shape', type: 'Option(Shape)', description: 'Border shape' },
    ],
  },
  {
    id: 'Modifier', name: 'Modifier', category: 'Custom Renderer', source: 'customRenderer.ts',
    definition: 'Enum({ margin, padding, background, border, height, width, minWidth, minHeight, fillWidth, fillHeight })',
    description: 'Layout and styling modifiers applied to custom renderer components.',
    variants: [
      { name: 'margin', type: 'Dimensions', description: 'Outer spacing' },
      { name: 'padding', type: 'Dimensions', description: 'Inner spacing' },
      { name: 'background', type: 'Struct({ color: ColorToken, shape: Option(Shape) })', description: 'Background fill' },
      { name: 'border', type: 'BorderStyle', description: 'Border style' },
      { name: 'height', type: 'Size', description: 'Fixed height' },
      { name: 'width', type: 'Size', description: 'Fixed width' },
      { name: 'minWidth', type: 'Size', description: 'Minimum width' },
      { name: 'minHeight', type: 'Size', description: 'Minimum height' },
      { name: 'fillWidth', type: 'bool', description: 'Fill available width' },
      { name: 'fillHeight', type: 'bool', description: 'Fill available height' },
    ],
  },
  {
    id: 'CustomRendererNode', name: 'CustomRendererNode', category: 'Custom Renderer', source: 'customRenderer.ts',
    definition: 'Enum({ Nil: _void, String: str, Box: Component<BoxProps>, Column: Component<ColumnProps>, Row: Component<RowProps>, Spacer: Component<_void>, Text: Component<TextProps>, Button: Component<ButtonProps>, TextField: Component<TextFieldProps> })',
    description: 'A node in the custom renderer UI tree. Can be nested recursively via children.',
    variants: [
      { name: 'Nil', type: '_void', description: 'Empty node' },
      { name: 'String', type: 'str', description: 'Raw text string' },
      { name: 'Box', type: 'Component<BoxProps>', description: 'Generic container' },
      { name: 'Column', type: 'Component<ColumnProps>', description: 'Vertical layout' },
      { name: 'Row', type: 'Component<RowProps>', description: 'Horizontal layout' },
      { name: 'Spacer', type: 'Component<_void>', description: 'Flexible space' },
      { name: 'Text', type: 'Component<TextProps>', description: 'Text display' },
      { name: 'Button', type: 'Component<ButtonProps>', description: 'Interactive button' },
      { name: 'TextField', type: 'Component<TextFieldProps>', description: 'Text input' },
    ],
  },
  // Chain Interaction Types
  { id: 'BlockHash', name: 'BlockHash', category: 'Chain', source: 'chainInteraction.ts', definition: 'Hex()', description: 'Block hash identifier.' },
  { id: 'OperationId', name: 'OperationId', category: 'Chain', source: 'chainInteraction.ts', definition: 'str', description: 'Operation identifier for async chain operations.' },
  {
    id: 'RuntimeApi', name: 'RuntimeApi', category: 'Chain', source: 'chainInteraction.ts',
    definition: 'Tuple(str, u32)',
    description: 'A runtime API identified by name and version.',
  },
  {
    id: 'RuntimeSpec', name: 'RuntimeSpec', category: 'Chain', source: 'chainInteraction.ts',
    definition: 'Struct({ specName, implName, specVersion, implVersion, transactionVersion?, apis })',
    description: 'Runtime specification metadata.',
    fields: [
      { name: 'specName', type: 'str', description: 'Specification name' },
      { name: 'implName', type: 'str', description: 'Implementation name' },
      { name: 'specVersion', type: 'u32', description: 'Spec version number' },
      { name: 'implVersion', type: 'u32', description: 'Implementation version' },
      { name: 'transactionVersion', type: 'Option(u32)', description: 'Transaction format version' },
      { name: 'apis', type: 'Vector(RuntimeApi)', description: 'Supported runtime APIs' },
    ],
  },
  {
    id: 'RuntimeType', name: 'RuntimeType', category: 'Chain', source: 'chainInteraction.ts',
    definition: 'Enum({ Valid: RuntimeSpec, Invalid: Struct({ error: str }) })',
    description: 'Runtime validity check result.',
    variants: [
      { name: 'Valid', type: 'RuntimeSpec', description: 'Valid runtime with spec' },
      { name: 'Invalid', type: 'Struct({ error: str })', description: 'Invalid runtime with error' },
    ],
  },
  {
    id: 'StorageQueryType', name: 'StorageQueryType', category: 'Chain', source: 'chainInteraction.ts',
    definition: "Status('Value', 'Hash', 'ClosestDescendantMerkleValue', 'DescendantsValues', 'DescendantsHashes')",
    description: 'Type of storage query to perform.',
  },
  {
    id: 'StorageQueryItem', name: 'StorageQueryItem', category: 'Chain', source: 'chainInteraction.ts',
    definition: 'Struct({ key: Hex(), type: StorageQueryType })',
    description: 'A single storage query.',
    fields: [
      { name: 'key', type: 'Hex()', description: 'Storage key to query' },
      { name: 'type', type: 'StorageQueryType', description: 'What to return' },
    ],
  },
  {
    id: 'StorageResultItem', name: 'StorageResultItem', category: 'Chain', source: 'chainInteraction.ts',
    definition: 'Struct({ key, value, hash, closestDescendantMerkleValue })',
    description: 'Result of a storage query.',
    fields: [
      { name: 'key', type: 'Hex()', description: 'The queried key' },
      { name: 'value', type: 'Nullable(Hex())', description: 'Value, if requested' },
      { name: 'hash', type: 'Nullable(Hex())', description: 'Hash, if requested' },
      { name: 'closestDescendantMerkleValue', type: 'Nullable(Hex())', description: 'Merkle value, if requested' },
    ],
  },
  {
    id: 'OperationStartedResult', name: 'OperationStartedResult', category: 'Chain', source: 'chainInteraction.ts',
    definition: 'Enum({ Started: Struct({ operationId: OperationId }), LimitReached: _void })',
    description: 'Result of starting a chain operation.',
    variants: [
      { name: 'Started', type: 'Struct({ operationId: OperationId })', description: 'Operation started successfully' },
      { name: 'LimitReached', type: '_void', description: 'Too many concurrent operations' },
    ],
  },
  {
    id: 'ChainHeadEvent', name: 'ChainHeadEvent', category: 'Chain', source: 'chainInteraction.ts',
    definition: 'Enum with 12 variants',
    description: 'Events received when following the chain head.',
    variants: [
      { name: 'Initialized', type: 'Struct({ finalizedBlockHashes, finalizedBlockRuntime? })', description: 'Initial state with finalized blocks' },
      { name: 'NewBlock', type: 'Struct({ blockHash, parentBlockHash, newRuntime? })', description: 'A new block was produced' },
      { name: 'BestBlockChanged', type: 'Struct({ bestBlockHash })', description: 'Best block changed' },
      { name: 'Finalized', type: 'Struct({ finalizedBlockHashes, prunedBlockHashes })', description: 'Blocks were finalized' },
      { name: 'OperationBodyDone', type: 'Struct({ operationId, value })', description: 'Body fetch completed' },
      { name: 'OperationCallDone', type: 'Struct({ operationId, output })', description: 'Runtime call completed' },
      { name: 'OperationStorageItems', type: 'Struct({ operationId, items })', description: 'Storage results batch' },
      { name: 'OperationStorageDone', type: 'Struct({ operationId })', description: 'Storage query completed' },
      { name: 'OperationWaitingForContinue', type: 'Struct({ operationId })', description: 'Operation paused, needs continue' },
      { name: 'OperationInaccessible', type: 'Struct({ operationId })', description: 'Block became inaccessible' },
      { name: 'OperationError', type: 'Struct({ operationId, error })', description: 'Operation failed' },
      { name: 'Stop', type: '_void', description: 'Subscription terminated by server' },
    ],
  },
  // Statement Store Types
  { id: 'Topic', name: 'Topic', category: 'Statement Store', source: 'statementStore.ts', definition: 'Bytes(32)', description: '32-byte topic identifier.' },
  { id: 'Channel', name: 'Channel', category: 'Statement Store', source: 'statementStore.ts', definition: 'Bytes(32)', description: '32-byte channel identifier.' },
  { id: 'DecryptionKey', name: 'DecryptionKey', category: 'Statement Store', source: 'statementStore.ts', definition: 'Bytes(32)', description: '32-byte decryption key.' },
  {
    id: 'StatementProof', name: 'StatementProof', category: 'Statement Store', source: 'statementStore.ts',
    definition: 'Enum({ Sr25519, Ed25519, Ecdsa, OnChain })',
    description: 'Cryptographic proof for a statement.',
    variants: [
      { name: 'Sr25519', type: 'Struct({ signature: Bytes(64), signer: Bytes(32) })', description: 'Sr25519 signature proof' },
      { name: 'Ed25519', type: 'Struct({ signature: Bytes(64), signer: Bytes(32) })', description: 'Ed25519 signature proof' },
      { name: 'Ecdsa', type: 'Struct({ signature: Bytes(65), signer: Bytes(33) })', description: 'ECDSA signature proof' },
      { name: 'OnChain', type: 'Struct({ who: Bytes(32), blockHash: Bytes(32), event: u64 })', description: 'On-chain event proof' },
    ],
  },
  {
    id: 'Statement', name: 'Statement', category: 'Statement Store', source: 'statementStore.ts',
    definition: 'Struct({ proof?, decryptionKey?, expiry?, channel?, topics, data? })',
    description: 'A statement with optional proof and metadata.',
    fields: [
      { name: 'proof', type: 'Option(StatementProof)', description: 'Optional cryptographic proof' },
      { name: 'decryptionKey', type: 'Option(DecryptionKey)', description: 'Optional decryption key' },
      { name: 'expiry', type: 'Option(u64)', description: 'Optional Unix timestamp expiry' },
      { name: 'channel', type: 'Option(Channel)', description: 'Optional channel' },
      { name: 'topics', type: 'Vector(Topic)', description: 'Topic tags' },
      { name: 'data', type: 'Option(Bytes())', description: 'Optional data payload' },
    ],
  },
  {
    id: 'SignedStatement', name: 'SignedStatement', category: 'Statement Store', source: 'statementStore.ts',
    definition: 'Struct({ proof, decryptionKey?, expiry?, channel?, topics, data? })',
    description: 'A statement with required (not optional) proof.',
    fields: [
      { name: 'proof', type: 'StatementProof', description: 'Required cryptographic proof' },
      { name: 'decryptionKey', type: 'Option(DecryptionKey)', description: 'Optional decryption key' },
      { name: 'expiry', type: 'Option(u64)', description: 'Optional Unix timestamp expiry' },
      { name: 'channel', type: 'Option(Channel)', description: 'Optional channel' },
      { name: 'topics', type: 'Vector(Topic)', description: 'Topic tags' },
      { name: 'data', type: 'Option(Bytes())', description: 'Optional data payload' },
    ],
  },
  {
    id: 'StatementProofErr', name: 'StatementProofErr', category: 'Statement Store', source: 'statementStore.ts',
    definition: 'ErrEnum { UnableToSign, UnknownAccount, Unknown({ reason: str }) }',
    description: 'Statement proof creation error.',
    variants: [
      { name: 'UnableToSign', type: '_void', description: 'Signing operation failed' },
      { name: 'UnknownAccount', type: '_void', description: 'Account not recognized' },
      { name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' },
    ],
  },
  // Preimage Types
  { id: 'PreimageKey', name: 'PreimageKey', category: 'Preimage', source: 'preimage.ts', definition: 'Hex()', description: 'Hash of the preimage.' },
  { id: 'PreimageValue', name: 'PreimageValue', category: 'Preimage', source: 'preimage.ts', definition: 'Bytes()', description: 'The preimage data.' },
  {
    id: 'PreimageSubmitErr', name: 'PreimageSubmitErr', category: 'Preimage', source: 'preimage.ts',
    definition: 'ErrEnum { Unknown({ reason: str }) }',
    description: 'Preimage submission error.',
    variants: [{ name: 'Unknown', type: 'Struct({ reason: str })', description: 'Catch-all' }],
  },
];

// Helper to extract type references from a string
const knownTypeNames = new Set(dataTypes.map(t => t.id));

export function extractTypeRefs(text: string): string[] {
  const refs: string[] = [];
  for (const id of knownTypeNames) {
    // Match the type name as a word boundary
    const regex = new RegExp(`\\b${id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`);
    if (regex.test(text)) {
      refs.push(id);
    }
  }
  return refs;
}

export function getTypeById(id: string): DataType | undefined {
  return dataTypes.find(t => t.id === id);
}

export function getMethodById(id: string): MethodDef | undefined {
  return methods.find(m => m.id === id);
}

export function getGroupById(id: string): GroupDef | undefined {
  return groups.find(g => g.id === id);
}

export function getTypeCategories(): string[] {
  const cats = new Set(dataTypes.map(t => t.category));
  return Array.from(cats);
}
