export interface MethodInfo {
  name: string;
  type: "unary" | "subscription";
  description?: string;
  requestDescription?: string;
  defaultRequest?: string;
  noParams?: boolean;
}

export interface ServiceInfo {
  name: string;
  methods: MethodInfo[];
}

const PASEO_GENESIS =
  "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";

export const services: ServiceInfo[] = [
  {
    name: "TrUAPI Calls",
    methods: [
      {
        name: "host_handshake",
        type: "unary",
        description:
          "Performs the protocol handshake. The product sends a protocol version number; the host accepts or rejects.",
        requestDescription: "Protocol version (u8)",
        defaultRequest: "1",
      },
      {
        name: "host_feature_supported",
        type: "unary",
        description:
          "Queries whether the host supports a specific feature. Currently only the Chain variant exists, carrying a genesis hash. Returns true if the chain is available.",
        requestDescription: "Feature enum, Chain(GenesisHash)",
        defaultRequest: `{ "tag": "Chain", "value": "${PASEO_GENESIS}" }`,
      },
      {
        name: "host_navigate_to",
        type: "unary",
        description:
          "Requests the host to open a URL, typically in a new browser tab.",
        requestDescription: "The URL to open",
        defaultRequest: '"https://example.com"',
      },
      {
        name: "host_push_notification",
        type: "unary",
        description:
          "Sends a push notification to the user via the host, with optional deeplink.",
        requestDescription:
          "PushNotification object: text and optional deeplink.",
        defaultRequest: '{ "text": "Hello!", "deeplink": null }',
      },
    ],
  },
  {
    name: "Permissions",
    methods: [
      {
        name: "host_device_permission",
        type: "unary",
        description:
          "Requests access to a device capability. Returns true if granted.",
        requestDescription:
          "HostDevicePermissionRequest enum: Camera | Microphone | Bluetooth | Location",
        defaultRequest: '{ "tag": "Camera" }',
      },
      {
        name: "remote_permission",
        type: "unary",
        description:
          "Requests permission for an ExternalRequest (a URL string) or TransactionSubmit. Returns true if granted.",
        requestDescription:
          "V01RemotePermissionRequest enum: ExternalRequest(str) | TransactionSubmit",
        defaultRequest:
          '{ "tag": "ExternalRequest", "value": "https://api.example.com" }',
      },
    ],
  },
  {
    name: "Local Storage",
    methods: [
      {
        name: "host_local_storage_read",
        type: "unary",
        description:
          "Reads bytes from the scoped key-value store by string key. Returns undefined if the key does not exist.",
        defaultRequest: '"test-key"',
      },
      {
        name: "host_local_storage_write",
        type: "unary",
        description:
          "Writes bytes to the scoped key-value store under a string key.",
        defaultRequest: '{ "key": "test-key", "value": "0x48656c6c6f" }',
      },
      {
        name: "host_local_storage_clear",
        type: "unary",
        description:
          "Removes the value at the given string key from the scoped key-value store.",
        defaultRequest: '"test-key"',
      },
    ],
  },
  {
    name: "Account Management",
    methods: [
      {
        name: "host_account_get",
        type: "unary",
        description:
          "Retrieves a product-specific derived account. The product provides a DotNS identifier and derivation index; the host returns the derived public key (and optional human-readable name) for that combination.",
        requestDescription:
          "ProductAccountId is a Tuple(DotNsIdentifier, DerivationIndex)",
        defaultRequest: '["truapi-playground.dot", 0]',
      },
      {
        name: "host_account_get_alias",
        type: "unary",
        description:
          "Retrieves a contextual alias (ring VRF based) for a product account, plus the context bytes used to derive it.",
        requestDescription:
          "ProductAccountId is a Tuple(DotNsIdentifier, DerivationIndex)",
        defaultRequest: '["truapi-playground.dot",0]',
      },
      {
        name: "host_account_create_proof",
        type: "unary",
        description:
          "Creates a ring VRF proof for a product account against a specific ring, signing the provided context bytes. Returns the proof bytes.",
        requestDescription: "ProductAccountId, RingLocation, and context bytes",
        defaultRequest: `{ "productAccountId": ["truapi-playground.dot", 0], "ringLocation": { "genesisHash": "${PASEO_GENESIS}", "ringRootHash": "${PASEO_GENESIS}", "hints": { "palletInstance": 42 } }, "context": "0x" }`,
      },
      {
        name: "host_get_non_product_accounts",
        type: "unary",
        description:
          "Retrieves the user's non-product accounts (e.g., their main wallet account, not derived per-product).",
        noParams: true,
      },
      {
        name: "host_account_connection_status_subscribe",
        type: "subscription",
        description:
          "Subscribes to changes in the user's authentication state. The host pushes 'connected' or 'disconnected' whenever the auth state changes.",
        noParams: true,
      },
      {
        name: "host_get_user_id",
        type: "unary",
        description:
          "Returns the user's primary account public key (and optional DotNS name). Requires JIT user approval on first call.",
        noParams: true,
      },
    ],
  },
  {
    name: "Signing",
    methods: [
      {
        name: "host_sign_payload",
        type: "unary",
        description:
          "Requests the host to sign a Substrate transaction payload. The host typically shows a confirmation modal. Returns the signature, plus the full signed transaction if requested.",
        requestDescription:
          "SigningPayload object: signer address (SS58 or hex), transaction metadata, method bytes, nonce, era, signed extensions, and tip.",
        defaultRequest: `{ "address": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY", "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000", "blockNumber": "0x00000000", "era": "0x00", "genesisHash": "${PASEO_GENESIS}", "method": "0x00000000", "nonce": "0x00000000", "signedExtensions": [], "specVersion": "0x00000000", "tip": "0x00000000000000000000000000000000", "transactionVersion": "0x00000000", "version": 4 }`,
      },
      {
        name: "host_sign_raw",
        type: "unary",
        description:
          "Requests the host to sign raw bytes or a string payload (not a transaction). Returns the signature.",
        requestDescription:
          "SigningRawPayload object: signer address (SS58 or hex) and raw data tagged as Bytes or Payload.",
        defaultRequest:
          '{ "address": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY", "data": { "tag": "Bytes", "value": "0x48656c6c6f" } }',
      },
      {
        name: "host_create_transaction",
        type: "unary",
        description:
          "Requests the host to create and sign a full transaction from a structured payload, using a product-derived account. Returns the signed transaction bytes.",
        requestDescription: "ProductAccountId and a VersionedTxPayload",
        defaultRequest:
          '{ "productAccountId": ["truapi-playground.dot", 0], "payload": { "tag": "V1", "value": { "signer": null, "callData": "0x0000", "extensions": [], "txExtVersion": 0, "context": { "metadata": "0x", "tokenSymbol": "DOT", "tokenDecimals": 10, "bestBlockHeight": 0 } } } }',
      },
      {
        name: "host_create_transaction_with_non_product_account",
        type: "unary",
        description:
          "Same as host_create_transaction but uses the user's main account instead of a product-derived account. Returns the signed transaction bytes.",
        requestDescription:
          "Same VersionedTxPayload structure, without ProductAccountId",
        defaultRequest:
          '{ "tag": "V1", "value": { "signer": null, "callData": "0x0000", "extensions": [], "txExtVersion": 0, "context": { "metadata": "0x", "tokenSymbol": "DOT", "tokenDecimals": 10, "bestBlockHeight": 0 } } }',
      },
    ],
  },
  {
    name: "Chat",
    methods: [
      {
        name: "host_chat_create_room",
        type: "unary",
        description:
          "Registers a chat room with the host. Returns whether the room is New or already Exists.",
        defaultRequest:
          '{ "roomId": "test-room", "name": "Test Room", "icon": "" }',
      },
      {
        name: "host_chat_register_bot",
        type: "unary",
        description:
          "Registers a bot identity for chat. Returns whether the bot is New or already Exists.",
        defaultRequest:
          '{ "botId": "test-bot", "name": "Test Bot", "icon": "" }',
      },
      {
        name: "host_chat_post_message",
        type: "unary",
        description:
          "Posts a message to a chat room. Payload is one of Text, RichText, Actions, File, Reaction, ReactionRemoved, or Custom. Returns the message id.",
        defaultRequest:
          '{ "roomId": "test-room", "payload": { "tag": "Text", "value": "Hello from playground!" } }',
      },
      {
        name: "host_chat_list_subscribe",
        type: "subscription",
        description:
          "Subscribes to the list of chat rooms the product participates in. The host pushes the full room list (each entry tagged participatingAs RoomHost or Bot) whenever it changes.",
        noParams: true,
      },
      {
        name: "host_chat_action_subscribe",
        type: "subscription",
        description:
          "Subscribes to chat actions: MessagePosted by peers, ActionTriggered (button clicks), and Command messages.",
        noParams: true,
      },
      {
        name: "product_chat_custom_message_render_subscribe",
        type: "subscription",
        description:
          "Reverse-direction subscription: the host initiates, asking the product to render a custom chat message as a UI tree of CustomRendererNode components.",
        requestDescription: "Host sends message details for product to render",
        noParams: true,
      },
      {
        name: "host_chat_create_simple_group",
        type: "unary",
        description:
          "Creates a simple group chat room. Participants join via the returned deep link. The host handles the group chat UI with default rendering (no custom elements).",
        requestDescription:
          "SimpleGroupChatRequest: roomId, name, and icon (URL or base64).",
        defaultRequest:
          '{ "roomId": "test-simple-group", "name": "Test Group", "icon": "" }',
      },
    ],
  },
  {
    name: "Statement Store",
    methods: [
      {
        name: "remote_statement_store_subscribe",
        type: "subscription",
        description:
          "Subscribes to statements matching a topic vector. The host pushes each matching signed statement as it arrives.",
        requestDescription:
          "Vec<Topic>: each entry is a 32-byte topic hex string. Empty array matches all statements.",
        defaultRequest: "[]",
      },
      {
        name: "remote_statement_store_create_proof",
        type: "unary",
        description:
          "Creates a cryptographic proof for a statement using a product account's key. Returns one of Sr25519, Ed25519, Ecdsa, or OnChain proof variants.",
        requestDescription: "ProductAccountId and a Statement to sign",
        defaultRequest:
          '{ "productAccountId": ["truapi-playground.dot", 0], "statement": { "proof": null, "decryptionKey": null, "expiry": "9999999999999n", "channel": null, "topics": [], "data": null } }',
      },
      {
        name: "remote_statement_store_submit",
        type: "unary",
        description:
          "Submits a SCALE-encoded signed statement to the statement store. Use remote_statement_store_create_proof to build a proof, then encode the final signed statement bytes before submitting.",
        requestDescription: "SCALE-encoded SignedStatement bytes (hex-encoded)",
        defaultRequest: '"0x"',
      },
    ],
  },
  {
    name: "Preimage",
    methods: [
      {
        name: "remote_preimage_lookup_subscribe",
        type: "subscription",
        description:
          "Subscribes to a preimage by its hash key. The host pushes the bytes when available, or null if the preimage is not known.",
        requestDescription:
          "The 32-byte Blake2b-256 hash of the preimage (hex-encoded)",
        defaultRequest:
          '"0x0000000000000000000000000000000000000000000000000000000000000000"',
      },
    ],
  },
  {
    name: "Chain Interaction",
    methods: [
      {
        name: "remote_chain_head_follow",
        type: "subscription",
        description:
          "Follows the chain head, receiving events about new blocks, finalization, and operation results. Implements the chainHead_v1_follow JSON-RPC method. The Subscription ID shown after subscribing is needed for the dependent methods (header, body, storage, call, unpin).",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "withRuntime": false }`,
      },
      {
        name: "remote_chain_head_header",
        type: "unary",
        description:
          "Retrieves a SCALE-encoded block header by hash within a follow subscription. Returns null if the hash is not pinned by the subscription.",
        requestDescription:
          "genesisHash, followSubscriptionId, and block hash. Leave followSubscriptionId empty and hash as zeros to have the playground open a one-shot follow and use the latest finalized block.",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "followSubscriptionId": "", "hash": "0x0000000000000000000000000000000000000000000000000000000000000000" }`,
      },
      {
        name: "remote_chain_head_body",
        type: "unary",
        description:
          "Retrieves a block body. Returns Started{operationId} (results arrive as OperationBodyDone events on the follow subscription) or LimitReached.",
        requestDescription:
          "genesisHash, followSubscriptionId, and block hash. Leave followSubscriptionId empty and hash as zeros to have the playground open a one-shot follow and use the latest finalized block.",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "followSubscriptionId": "", "hash": "0x0000000000000000000000000000000000000000000000000000000000000000" }`,
      },
      {
        name: "remote_chain_head_storage",
        type: "unary",
        description:
          "Queries chain storage. Returns Started{operationId} (results arrive as OperationStorageItems and OperationStorageDone events) or LimitReached.",
        requestDescription:
          "genesisHash, followSubscriptionId, block hash, storage items, and optional childTrie. Leave followSubscriptionId empty and hash as zeros to use a one-shot follow against the latest finalized block.",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "followSubscriptionId": "", "hash": "0x0000000000000000000000000000000000000000000000000000000000000000", "items": [{ "key": "0x26aa394eea5630e07c48ae0c9558cef7", "type": "Value" }], "childTrie": null }`,
      },
      {
        name: "remote_chain_head_call",
        type: "unary",
        description:
          "Executes a runtime API call. Returns Started{operationId} (result arrives as an OperationCallDone event) or LimitReached.",
        requestDescription:
          "genesisHash, followSubscriptionId, block hash, runtime function name, and call parameters. Leave followSubscriptionId empty and hash as zeros for an automatic one-shot follow.",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "followSubscriptionId": "", "hash": "0x0000000000000000000000000000000000000000000000000000000000000000", "function": "Core_version", "callParameters": "0x" }`,
      },
      {
        name: "remote_chain_head_unpin",
        type: "unary",
        description: "Unpins block hashes, allowing the node to discard them.",
        requestDescription:
          "genesisHash, followSubscriptionId, and array of block hashes to unpin. Leave followSubscriptionId empty and hashes as zeros for an automatic one-shot follow.",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "followSubscriptionId": "", "hashes": ["0x0000000000000000000000000000000000000000000000000000000000000000"] }`,
      },
      {
        name: "remote_chain_head_continue",
        type: "unary",
        description:
          "Continues a paused operation (when OperationWaitingForContinue is received).",
        requestDescription:
          "genesisHash, followSubscriptionId, and operationId from an OperationWaitingForContinue event. Needs a real operationId — the auto-follow fills in the subscription id but cannot conjure an operation.",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "followSubscriptionId": "", "operationId": "op-id" }`,
      },
      {
        name: "remote_chain_head_stop_operation",
        type: "unary",
        description: "Stops an in-progress operation.",
        requestDescription:
          "genesisHash, followSubscriptionId, and operationId to stop. Needs a real operationId — the auto-follow fills in the subscription id but cannot conjure an operation.",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "followSubscriptionId": "", "operationId": "op-id" }`,
      },
      {
        name: "remote_chain_spec_genesis_hash",
        type: "unary",
        description: "Gets the genesis hash for a chain.",
        defaultRequest: `"${PASEO_GENESIS}"`,
      },
      {
        name: "remote_chain_spec_chain_name",
        type: "unary",
        description: "Gets the chain name.",
        defaultRequest: `"${PASEO_GENESIS}"`,
      },
      {
        name: "remote_chain_spec_properties",
        type: "unary",
        description: "Gets the chain properties as a JSON-encoded string.",
        defaultRequest: `"${PASEO_GENESIS}"`,
      },
      {
        name: "remote_chain_transaction_broadcast",
        type: "unary",
        description:
          "Broadcasts a signed transaction to the network. Returns an operationId for cancellation, or null if broadcast tracking is not supported.",
        requestDescription:
          "genesisHash and the signed transaction bytes (hex-encoded)",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "transaction": "0x" }`,
      },
      {
        name: "remote_chain_transaction_stop",
        type: "unary",
        description: "Stops broadcasting a transaction.",
        requestDescription:
          "genesisHash and the operationId returned by transaction_broadcast",
        defaultRequest: `{ "genesisHash": "${PASEO_GENESIS}", "operationId": "op-id" }`,
      },
    ],
  },
  {
    name: "Payment",
    methods: [
      {
        name: "host_payment_balance_subscribe",
        type: "subscription",
        description:
          "Subscribes to the user's payment balance. The host prompts the user for permission to disclose their balance on the first call. Pushes PaymentBalance updates with the available amount.",
        noParams: true,
      },
      {
        name: "host_payment_top_up",
        type: "unary",
        description:
          "Tops up the user's payment balance from a product-controlled funding source (ProductAccount or PrivateKey). This operation is always in the user's favour and does not require user consent.",
        requestDescription:
          "PaymentTopUpRequest: amount (Balance, u128) and source (PaymentTopUpSource enum: ProductAccount(ProductAccountId) | PrivateKey([u8; 32])).",
        defaultRequest:
          '{ "amount": "0n", "source": { "tag": "ProductAccount", "value": ["truapi-playground.dot", 0] } }',
      },
      {
        name: "host_payment_request",
        type: "unary",
        description:
          "Requests a payment from the user's available balance to a destination account. The host prompts the user to authorize. Returns a payment id (track via host_payment_status_subscribe); a successful response means the host accepted the payment for processing, not that it has settled.",
        requestDescription:
          "PaymentRequestRequest: amount (Balance, u128) and destination (32-byte AccountId).",
        defaultRequest:
          '{ "amount": "0n", "destination": "0x0000000000000000000000000000000000000000000000000000000000000000" }',
      },
      {
        name: "host_payment_status_subscribe",
        type: "subscription",
        description:
          "Subscribes to status updates for a previously requested payment. Emits Processing, then a terminal Completed or Failed (which carries an error reason).",
        requestDescription:
          "PaymentId returned by host_payment_request (string).",
        defaultRequest: '"payment-id"',
      },
    ],
  },
  {
    name: "Entropy Derivation",
    methods: [
      {
        name: "host_derive_entropy",
        type: "unary",
        description:
          "Derives 32 bytes of deterministic entropy scoped to the calling product and the provided key. Uses a three-layer BLAKE2b-256 keyed hashing scheme over the user's root BIP-39 entropy. The same root account + product + key always yields the same output on any conforming host.",
        requestDescription:
          "Arbitrary key bytes (up to 32 bytes) chosen by the caller; hex-encoded.",
        defaultRequest: '"0x"',
      },
    ],
  },
];
