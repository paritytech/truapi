export const manifest = {
  "schemaVersion": 1,
  "protocol": {
    "name": "TrUAPI",
    "version": "0.2",
    "source": {
      "repo": "https://github.com/paritytech/truapi",
      "path": "truapi-spec/src/v02/mod.rs",
      "revision": "a7fa645"
    },
    "transport": "message-port",
    "wireFormat": "scale-host-api"
  },
  "methods": [
    {
      "name": "host_feature_supported",
      "tag": 0,
      "kind": "request",
      "group": "truapi-calls",
      "request": "Feature",
      "response": "Result(bool, GenericError)",
      "errorType": null
    },
    {
      "name": "host_navigate_to",
      "tag": 1,
      "kind": "request",
      "group": "truapi-calls",
      "request": "str",
      "response": "Result(void, NavigateToErr)",
      "errorType": "NavigateToErr"
    },
    {
      "name": "host_push_notification",
      "tag": 2,
      "kind": "request",
      "group": "truapi-calls",
      "request": "PushNotification",
      "response": "Result(void, GenericError)",
      "errorType": null
    },
    {
      "name": "host_device_permission",
      "tag": 3,
      "kind": "request",
      "group": "permissions",
      "request": "DevicePermission",
      "response": "Result(bool, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_permission",
      "tag": 4,
      "kind": "request",
      "group": "permissions",
      "request": "Vector(RemotePermission)",
      "response": "Result(bool, GenericError)",
      "errorType": null
    },
    {
      "name": "host_local_storage_read",
      "tag": 5,
      "kind": "request",
      "group": "local-storage",
      "request": "StorageKey",
      "response": "Result(Option(StorageValue), StorageErr)",
      "errorType": "StorageErr"
    },
    {
      "name": "host_local_storage_write",
      "tag": 6,
      "kind": "request",
      "group": "local-storage",
      "request": "Tuple(StorageKey, StorageValue)",
      "response": "Result(void, StorageErr)",
      "errorType": "StorageErr"
    },
    {
      "name": "host_local_storage_clear",
      "tag": 7,
      "kind": "request",
      "group": "local-storage",
      "request": "StorageKey",
      "response": "Result(void, StorageErr)",
      "errorType": "StorageErr"
    },
    {
      "name": "host_account_get",
      "tag": 8,
      "kind": "request",
      "group": "account-management",
      "request": "ProductAccountId",
      "response": "Result(ProductAccount, RequestCredentialsErr)",
      "errorType": "RequestCredentialsErr"
    },
    {
      "name": "host_account_get_alias",
      "tag": 9,
      "kind": "request",
      "group": "account-management",
      "request": "ProductAccountId",
      "response": "Result(ContextualAlias, RequestCredentialsErr)",
      "errorType": "RequestCredentialsErr"
    },
    {
      "name": "host_account_create_proof",
      "tag": 10,
      "kind": "request",
      "group": "account-management",
      "request": "Tuple(ProductAccountId, RingLocation, Bytes)",
      "response": "Result(RingVrfProof, CreateProofErr)",
      "errorType": "CreateProofErr"
    },
    {
      "name": "host_get_legacy_accounts",
      "tag": 11,
      "kind": "request",
      "group": "account-management",
      "request": "void",
      "response": "Result(Vector(LegacyAccount), RequestCredentialsErr)",
      "errorType": "RequestCredentialsErr"
    },
    {
      "name": "host_account_connection_status_subscribe",
      "tag": 12,
      "kind": "subscription",
      "group": "account-management",
      "request": "void",
      "response": "AccountConnectionStatus",
      "errorType": null
    },
    {
      "name": "host_get_user_id",
      "tag": 13,
      "kind": "request",
      "group": "account-management",
      "request": "void",
      "response": "Result(UserIdentity, UserIdentityErr)",
      "errorType": "UserIdentityErr"
    },
    {
      "name": "host_request_login",
      "tag": 14,
      "kind": "request",
      "group": "account-management",
      "request": "Option(str)",
      "response": "Result(LoginResult, LoginError)",
      "errorType": "LoginError"
    },
    {
      "name": "host_sign_payload",
      "tag": 15,
      "kind": "request",
      "group": "signing",
      "request": "SigningPayload",
      "response": "Result(SigningResult, SigningErr)",
      "errorType": "SigningErr"
    },
    {
      "name": "host_sign_raw",
      "tag": 16,
      "kind": "request",
      "group": "signing",
      "request": "SigningRawPayload",
      "response": "Result(SigningResult, SigningErr)",
      "errorType": "SigningErr"
    },
    {
      "name": "host_sign_raw_with_legacy_account",
      "tag": 17,
      "kind": "request",
      "group": "signing",
      "request": "SigningRawPayloadWithoutAccount",
      "response": "Result(SigningResult, SigningErr)",
      "errorType": "SigningErr"
    },
    {
      "name": "host_sign_payload_with_legacy_account",
      "tag": 18,
      "kind": "request",
      "group": "signing",
      "request": "SigningPayloadWithoutAccount",
      "response": "Result(SigningResult, SigningErr)",
      "errorType": "SigningErr"
    },
    {
      "name": "host_create_transaction",
      "tag": 19,
      "kind": "request",
      "group": "signing",
      "request": "Tuple(ProductAccountId, VersionedTxPayload)",
      "response": "Result(Bytes, CreateTransactionErr)",
      "errorType": "CreateTransactionErr"
    },
    {
      "name": "host_create_transaction_with_legacy_account",
      "tag": 20,
      "kind": "request",
      "group": "signing",
      "request": "VersionedTxPayload",
      "response": "Result(Bytes, CreateTransactionErr)",
      "errorType": "CreateTransactionErr"
    },
    {
      "name": "host_chat_create_room",
      "tag": 21,
      "kind": "request",
      "group": "chat",
      "request": "ChatRoomRequest",
      "response": "Result(ChatRoomRegistrationResult, ChatRoomRegistrationErr)",
      "errorType": "ChatRoomRegistrationErr"
    },
    {
      "name": "host_chat_create_simple_group",
      "tag": 22,
      "kind": "request",
      "group": "chat",
      "request": "SimpleGroupChatRequest",
      "response": "Result(SimpleGroupChatResult, ChatRoomRegistrationErr)",
      "errorType": "ChatRoomRegistrationErr"
    },
    {
      "name": "host_chat_register_bot",
      "tag": 23,
      "kind": "request",
      "group": "chat",
      "request": "ChatBotRequest",
      "response": "Result(ChatBotRegistrationResult, ChatBotRegistrationErr)",
      "errorType": "ChatBotRegistrationErr"
    },
    {
      "name": "host_chat_post_message",
      "tag": 24,
      "kind": "request",
      "group": "chat",
      "request": "Struct { roomId: str, payload: ChatMessageContent }",
      "response": "Result(ChatPostMessageResult, ChatMessagePostingErr)",
      "errorType": "ChatMessagePostingErr"
    },
    {
      "name": "host_chat_list_subscribe",
      "tag": 25,
      "kind": "subscription",
      "group": "chat",
      "request": "void",
      "response": "Vector(ChatRoom)",
      "errorType": null
    },
    {
      "name": "host_chat_action_subscribe",
      "tag": 26,
      "kind": "subscription",
      "group": "chat",
      "request": "void",
      "response": "ReceivedChatAction",
      "errorType": null
    },
    {
      "name": "product_chat_custom_message_render_subscribe",
      "tag": 27,
      "kind": "reverse-subscription",
      "group": "chat",
      "request": "Struct { messageId: str, messageType: str, payload: Bytes }",
      "response": "CustomRendererNode",
      "errorType": null
    },
    {
      "name": "remote_statement_store_subscribe",
      "tag": 28,
      "kind": "subscription",
      "group": "statement-store",
      "request": "TopicFilter",
      "response": "SignedStatementsPage",
      "errorType": null
    },
    {
      "name": "remote_statement_store_create_proof",
      "tag": 29,
      "kind": "request",
      "group": "statement-store",
      "request": "Tuple(ProductAccountId, Statement)",
      "response": "Result(StatementProof, StatementProofErr)",
      "errorType": "StatementProofErr"
    },
    {
      "name": "remote_statement_store_submit",
      "tag": 30,
      "kind": "request",
      "group": "statement-store",
      "request": "SignedStatement",
      "response": "Result(void, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_preimage_lookup_subscribe",
      "tag": 31,
      "kind": "subscription",
      "group": "preimage",
      "request": "PreimageKey",
      "response": "Nullable(PreimageValue)",
      "errorType": null
    },
    {
      "name": "remote_chain_head_follow",
      "tag": 32,
      "kind": "subscription",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, withRuntime: bool }",
      "response": "ChainHeadEvent",
      "errorType": null
    },
    {
      "name": "remote_chain_head_header",
      "tag": 33,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, followSubscriptionId: str, hash: BlockHash }",
      "response": "Result(Nullable(Hex), GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_head_body",
      "tag": 34,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, followSubscriptionId: str, hash: BlockHash }",
      "response": "Result(OperationStartedResult, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_head_storage",
      "tag": 35,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, followSubscriptionId: str, hash: BlockHash, items: Vector(StorageQueryItem), childTrie: Nullable(Hex) }",
      "response": "Result(OperationStartedResult, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_head_call",
      "tag": 36,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, followSubscriptionId: str, hash: BlockHash, function: str, callParameters: Hex }",
      "response": "Result(OperationStartedResult, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_head_unpin",
      "tag": 37,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, followSubscriptionId: str, hashes: Vector(BlockHash) }",
      "response": "Result(void, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_head_continue",
      "tag": 38,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, followSubscriptionId: str, operationId: OperationId }",
      "response": "Result(void, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_head_stop_operation",
      "tag": 39,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, followSubscriptionId: str, operationId: OperationId }",
      "response": "Result(void, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_spec_genesis_hash",
      "tag": 40,
      "kind": "request",
      "group": "chain-interaction",
      "request": "GenesisHash",
      "response": "Result(Hex, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_spec_chain_name",
      "tag": 41,
      "kind": "request",
      "group": "chain-interaction",
      "request": "GenesisHash",
      "response": "Result(str, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_spec_properties",
      "tag": 42,
      "kind": "request",
      "group": "chain-interaction",
      "request": "GenesisHash",
      "response": "Result(str, GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_transaction_broadcast",
      "tag": 43,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, transaction: Hex }",
      "response": "Result(Nullable(str), GenericError)",
      "errorType": null
    },
    {
      "name": "remote_chain_transaction_stop",
      "tag": 44,
      "kind": "request",
      "group": "chain-interaction",
      "request": "Struct { genesisHash: GenesisHash, operationId: OperationId }",
      "response": "Result(void, GenericError)",
      "errorType": null
    },
    {
      "name": "host_payment_balance_subscribe",
      "tag": 45,
      "kind": "subscription",
      "group": "payment",
      "request": "void",
      "response": "PaymentBalance",
      "errorType": "PaymentBalanceErr"
    },
    {
      "name": "host_payment_top_up",
      "tag": 46,
      "kind": "request",
      "group": "payment",
      "request": "Tuple(Balance, PaymentTopUpSource)",
      "response": "Result(void, PaymentTopUpErr)",
      "errorType": "PaymentTopUpErr"
    },
    {
      "name": "host_payment_request",
      "tag": 47,
      "kind": "request",
      "group": "payment",
      "request": "Tuple(Balance, AccountId)",
      "response": "Result(PaymentReceipt, PaymentRequestErr)",
      "errorType": "PaymentRequestErr"
    },
    {
      "name": "host_payment_status_subscribe",
      "tag": 48,
      "kind": "subscription",
      "group": "payment",
      "request": "PaymentId",
      "response": "PaymentStatus",
      "errorType": "PaymentStatusErr"
    },
    {
      "name": "host_derive_entropy",
      "tag": 49,
      "kind": "request",
      "group": "entropy",
      "request": "Vector(u8)",
      "response": "Result(Entropy, DeriveEntropyErr)",
      "errorType": "DeriveEntropyErr"
    },
    {
      "name": "host_theme_subscribe",
      "tag": 50,
      "kind": "subscription",
      "group": "theme",
      "request": "void",
      "response": "Theme",
      "errorType": null
    }
  ],
  "groups": [
    {
      "id": "truapi-calls",
      "name": "TrUAPI Calls",
      "description": "General-purpose TrUAPI methods for feature detection, navigation, notifications, and permissions.",
      "methods": [
        "host_feature_supported",
        "host_navigate_to",
        "host_push_notification"
      ]
    },
    {
      "id": "permissions",
      "name": "Permissions",
      "description": "Device and remote permission requests for camera, microphone, HTTP, and transaction access.",
      "methods": [
        "host_device_permission",
        "remote_permission"
      ]
    },
    {
      "id": "local-storage",
      "name": "Local Storage",
      "description": "Scoped key-value storage. The host namespaces keys so different products cannot read each other's data.",
      "methods": [
        "host_local_storage_read",
        "host_local_storage_write",
        "host_local_storage_clear"
      ]
    },
    {
      "id": "account-management",
      "name": "Account Management",
      "description": "Product-specific account derivation, alias retrieval, ring VRF proofs, connection status, and user identity.",
      "methods": [
        "host_account_get",
        "host_account_get_alias",
        "host_account_create_proof",
        "host_get_legacy_accounts",
        "host_account_connection_status_subscribe",
        "host_get_user_id",
        "host_request_login"
      ]
    },
    {
      "id": "signing",
      "name": "Signing",
      "description": "Transaction payload signing, raw message signing, and full transaction creation.",
      "methods": [
        "host_sign_payload",
        "host_sign_raw",
        "host_create_transaction",
        "host_sign_raw_with_legacy_account",
        "host_sign_payload_with_legacy_account",
        "host_create_transaction_with_legacy_account"
      ]
    },
    {
      "id": "chat",
      "name": "Chat",
      "description": "Chat room management, bot registration, message posting, simple group chats, and custom message rendering.",
      "methods": [
        "host_chat_create_room",
        "host_chat_register_bot",
        "host_chat_post_message",
        "host_chat_list_subscribe",
        "host_chat_action_subscribe",
        "product_chat_custom_message_render_subscribe",
        "host_chat_create_simple_group"
      ]
    },
    {
      "id": "statement-store",
      "name": "Statement Store",
      "description": "Subscribe to, create proofs for, and submit cryptographic statements.",
      "methods": [
        "remote_statement_store_subscribe",
        "remote_statement_store_create_proof",
        "remote_statement_store_submit"
      ]
    },
    {
      "id": "preimage",
      "name": "Preimage",
      "description": "Lookup preimages by hash.",
      "methods": [
        "remote_preimage_lookup_subscribe"
      ]
    },
    {
      "id": "chain-interaction",
      "name": "Chain Interaction",
      "description": "Substrate blockchain RPC access implementing the chainHead v1 JSON-RPC spec over binary protocol.",
      "methods": [
        "remote_chain_head_follow",
        "remote_chain_head_header",
        "remote_chain_head_body",
        "remote_chain_head_storage",
        "remote_chain_head_call",
        "remote_chain_head_unpin",
        "remote_chain_head_continue",
        "remote_chain_head_stop_operation",
        "remote_chain_spec_genesis_hash",
        "remote_chain_spec_chain_name",
        "remote_chain_spec_properties",
        "remote_chain_transaction_broadcast",
        "remote_chain_transaction_stop"
      ]
    },
    {
      "id": "payment",
      "name": "Payment",
      "description": "Coinage API for balance subscriptions, top-ups, payment requests, and payment status tracking.",
      "methods": [
        "host_payment_balance_subscribe",
        "host_payment_top_up",
        "host_payment_request",
        "host_payment_status_subscribe"
      ]
    },
    {
      "id": "entropy",
      "name": "Entropy",
      "description": "Deterministic entropy derivation scoped to product and user via BLAKE2b-256 keyed hashing.",
      "methods": [
        "host_derive_entropy"
      ]
    },
    {
      "id": "theme",
      "name": "Theme",
      "description": "Host visual theme subscription.",
      "methods": [
        "host_theme_subscribe"
      ]
    }
  ],
  "dataTypes": [
    {
      "id": "str",
      "name": "str",
      "category": "Primitives",
      "source": null,
      "definition": "length-prefixed UTF-8",
      "description": "String value, SCALE-encoded as length-prefixed UTF-8 bytes.",
      "fields": [],
      "variants": []
    },
    {
      "id": "bool",
      "name": "bool",
      "category": "Primitives",
      "source": null,
      "definition": "single byte",
      "description": "Boolean value encoded as a single byte (0x00 = false, 0x01 = true).",
      "fields": [],
      "variants": []
    },
    {
      "id": "u8",
      "name": "u8",
      "category": "Primitives",
      "source": null,
      "definition": "1 byte unsigned",
      "description": "8-bit unsigned integer.",
      "fields": [],
      "variants": []
    },
    {
      "id": "u32",
      "name": "u32",
      "category": "Primitives",
      "source": null,
      "definition": "4 bytes LE unsigned",
      "description": "32-bit unsigned integer, little-endian encoded.",
      "fields": [],
      "variants": []
    },
    {
      "id": "u64",
      "name": "u64",
      "category": "Primitives",
      "source": null,
      "definition": "8 bytes LE unsigned",
      "description": "64-bit unsigned integer, little-endian encoded.",
      "fields": [],
      "variants": []
    },
    {
      "id": "u128",
      "name": "u128",
      "category": "Primitives",
      "source": null,
      "definition": "16 bytes LE unsigned",
      "description": "128-bit unsigned integer, little-endian encoded.",
      "fields": [],
      "variants": []
    },
    {
      "id": "compact",
      "name": "compact",
      "category": "Primitives",
      "source": null,
      "definition": "SCALE compact integer",
      "description": "Variable-length unsigned integer using SCALE compact encoding.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Hex",
      "name": "Hex / Hex()",
      "category": "Primitives",
      "source": null,
      "definition": "length-prefixed bytes",
      "description": "Arbitrary hex-encoded bytes, SCALE length-prefixed.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Bytes",
      "name": "Bytes()",
      "category": "Primitives",
      "source": null,
      "definition": "length-prefixed bytes",
      "description": "Arbitrary binary data, SCALE length-prefixed.",
      "fields": [],
      "variants": []
    },
    {
      "id": "BytesN",
      "name": "Bytes(N)",
      "category": "Primitives",
      "source": null,
      "definition": "fixed N bytes",
      "description": "Fixed-length binary data of exactly N bytes.",
      "fields": [],
      "variants": []
    },
    {
      "id": "_void",
      "name": "_void",
      "category": "Primitives",
      "source": null,
      "definition": "zero bytes",
      "description": "Unit type / no data. Takes zero bytes on the wire.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Option",
      "name": "Option(T)",
      "category": "Combinators",
      "source": null,
      "definition": "None (0x00) or Some(T) (0x01 + encoded T)",
      "description": "Optional value. Encoded as 0x00 for None, or 0x01 followed by the encoded inner value.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Nullable",
      "name": "Nullable(T)",
      "category": "Combinators",
      "source": null,
      "definition": "Null or T",
      "description": "Similar to Option but with different encoding semantics for null values.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Vector",
      "name": "Vector(T)",
      "category": "Combinators",
      "source": null,
      "definition": "Length-prefixed array of T",
      "description": "A variable-length array. Encoded as a compact length prefix followed by each element.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Tuple",
      "name": "Tuple(A, B, ...)",
      "category": "Combinators",
      "source": null,
      "definition": "Concatenated encodings of A, B, ...",
      "description": "Fixed-size collection of values of potentially different types, encoded by concatenation.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Struct",
      "name": "Struct({ k: T, ... })",
      "category": "Combinators",
      "source": null,
      "definition": "Concatenated encodings of fields in definition order",
      "description": "Named fields encoded in declaration order by concatenation.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Enum",
      "name": "Enum({ V1: T1, V2: T2, ... })",
      "category": "Combinators",
      "source": null,
      "definition": "Tag byte + variant encoding",
      "description": "Tagged union. A single tag byte selects the variant, followed by that variant's encoding.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Status",
      "name": "Status(s1, s2, ...)",
      "category": "Combinators",
      "source": null,
      "definition": "Enum where each variant carries _void",
      "description": "Enumeration of named states, each carrying no data (all variants are _void).",
      "fields": [],
      "variants": []
    },
    {
      "id": "Result",
      "name": "Result(Ok, Err)",
      "category": "Combinators",
      "source": null,
      "definition": "0x00 + Ok encoding, or 0x01 + Err encoding",
      "description": "Success/failure wrapper. 0x00 prefix for Ok, 0x01 prefix for Err.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ErrEnum",
      "name": "ErrEnum(name, variants)",
      "category": "Combinators",
      "source": null,
      "definition": "Error enum with descriptive variant names",
      "description": "Specialized enum used for error types with human-readable variant names.",
      "fields": [],
      "variants": []
    },
    {
      "id": "GenesisHash",
      "name": "GenesisHash",
      "category": "Common",
      "source": "commonCodecs.ts",
      "definition": "Hex()",
      "description": "Blockchain genesis hash, used to identify a specific chain.",
      "fields": [],
      "variants": []
    },
    {
      "id": "GenericErr",
      "name": "GenericErr",
      "category": "Common",
      "source": "commonCodecs.ts",
      "definition": "Struct({ reason: str })",
      "description": "Generic error payload carrying a human-readable reason string.",
      "fields": [],
      "variants": []
    },
    {
      "id": "GenericError",
      "name": "GenericError",
      "category": "Common",
      "source": "commonCodecs.ts",
      "definition": "ErrEnum { GenericError(GenericErr) }",
      "description": "Single-variant error enum wrapping GenericErr. Used by many methods as a catch-all error type.",
      "fields": [],
      "variants": []
    },
    {
      "id": "AccountId",
      "name": "AccountId",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Bytes(32)",
      "description": "32-byte account identifier (typically an SS58 public key).",
      "fields": [],
      "variants": []
    },
    {
      "id": "PublicKey",
      "name": "PublicKey",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Bytes()",
      "description": "Variable-length public key.",
      "fields": [],
      "variants": []
    },
    {
      "id": "DotNsIdentifier",
      "name": "DotNsIdentifier",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "str",
      "description": "A dotNS domain name identifier (e.g., \"my-product.dot\").",
      "fields": [],
      "variants": []
    },
    {
      "id": "DerivationIndex",
      "name": "DerivationIndex",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "u32",
      "description": "Key derivation index for generating product-specific accounts.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ProductAccountId",
      "name": "ProductAccountId",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Tuple(DotNsIdentifier, DerivationIndex)",
      "description": "Identifies a product-specific account by combining a dotNS domain name with a derivation index.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ProductAccount",
      "name": "ProductAccount",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Struct({ publicKey: PublicKey })",
      "description": "A protocol-derived, product-scoped account. No user-chosen label.",
      "fields": [
        {
          "name": "publicKey",
          "type": "PublicKey",
          "description": "The account public key (variable-length Bytes)"
        }
      ],
      "variants": []
    },
    {
      "id": "LegacyAccount",
      "name": "LegacyAccount",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Struct({ publicKey: PublicKey, name: Option(str) })",
      "description": "A user-imported account. May carry a user-chosen label.",
      "fields": [
        {
          "name": "publicKey",
          "type": "PublicKey",
          "description": "The account public key (variable-length Bytes)"
        },
        {
          "name": "name",
          "type": "Option(str)",
          "description": "Optional human-readable display name"
        }
      ],
      "variants": []
    },
    {
      "id": "ContextualAlias",
      "name": "ContextualAlias",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Struct({ context: Bytes(32), alias: Bytes() })",
      "description": "A privacy-preserving alias derived via ring VRF, bound to a specific context.",
      "fields": [
        {
          "name": "context",
          "type": "Bytes(32)",
          "description": "32-byte context identifier"
        },
        {
          "name": "alias",
          "type": "Bytes()",
          "description": "Ring VRF alias (variable length)"
        }
      ],
      "variants": []
    },
    {
      "id": "RingLocationHint",
      "name": "RingLocationHint",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Struct({ palletInstance: Option(u32) })",
      "description": "Hints for locating a ring on-chain.",
      "fields": [
        {
          "name": "palletInstance",
          "type": "Option(u32)",
          "description": "Optional pallet instance index"
        }
      ],
      "variants": []
    },
    {
      "id": "RingLocation",
      "name": "RingLocation",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Struct({ genesisHash: GenesisHash, ringRootHash: Hex(), hints: Option(RingLocationHint) })",
      "description": "Locates a specific ring on a specific chain for ring VRF operations.",
      "fields": [
        {
          "name": "genesisHash",
          "type": "GenesisHash",
          "description": "Chain genesis hash"
        },
        {
          "name": "ringRootHash",
          "type": "Hex()",
          "description": "Root hash of the ring"
        },
        {
          "name": "hints",
          "type": "Option(RingLocationHint)",
          "description": "Optional location hints"
        }
      ],
      "variants": []
    },
    {
      "id": "RingVrfProof",
      "name": "RingVrfProof",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Bytes()",
      "description": "Variable-length ring VRF proof bytes.",
      "fields": [],
      "variants": []
    },
    {
      "id": "AccountConnectionStatus",
      "name": "AccountConnectionStatus",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Status('disconnected', 'connected')",
      "description": "Status enum representing the user's authentication state.",
      "fields": [],
      "variants": []
    },
    {
      "id": "LoginResult",
      "name": "LoginResult",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Status('Success', 'AlreadyConnected', 'Rejected')",
      "description": "Outcome of a login request.",
      "fields": [],
      "variants": []
    },
    {
      "id": "LoginError",
      "name": "LoginError",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "ErrEnum { Unknown({ reason: str }) }",
      "description": "Error from host_request_login.",
      "fields": [],
      "variants": [
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "UserIdentity",
      "name": "UserIdentity",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "Struct({ primaryUsername: DotNsIdentifier, publicKey: PublicKey })",
      "description": "The user's primary DotNS account identity. V0.2 addition.",
      "fields": [
        {
          "name": "primaryUsername",
          "type": "DotNsIdentifier",
          "description": "The user's primary DotNS username"
        },
        {
          "name": "publicKey",
          "type": "PublicKey",
          "description": "The user's primary public key"
        }
      ],
      "variants": []
    },
    {
      "id": "UserIdentityErr",
      "name": "UserIdentityErr",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "ErrEnum { PermissionDenied, NotConnected, Unknown({ reason: str }) }",
      "description": "Error from host_get_user_id. V0.2 addition.",
      "fields": [],
      "variants": [
        {
          "name": "PermissionDenied",
          "type": "_void",
          "description": "User denied the identity disclosure request"
        },
        {
          "name": "NotConnected",
          "type": "_void",
          "description": "User is not logged in"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "RequestCredentialsErr",
      "name": "RequestCredentialsErr",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "ErrEnum { NotConnected, Rejected, DomainNotValid, Unknown({ reason: str }) }",
      "description": "Error returned when credential/account requests fail.",
      "fields": [],
      "variants": [
        {
          "name": "NotConnected",
          "type": "_void",
          "description": "User is not logged in"
        },
        {
          "name": "Rejected",
          "type": "_void",
          "description": "User or host rejected the request"
        },
        {
          "name": "DomainNotValid",
          "type": "_void",
          "description": "Domain identifier is invalid"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all error with reason"
        }
      ]
    },
    {
      "id": "CreateProofErr",
      "name": "CreateProofErr",
      "category": "Account",
      "source": "accounts.ts",
      "definition": "ErrEnum { RingNotFound, Rejected, Unknown({ reason: str }) }",
      "description": "Error returned when ring VRF proof creation fails.",
      "fields": [],
      "variants": [
        {
          "name": "RingNotFound",
          "type": "_void",
          "description": "Ring not available at the specified location"
        },
        {
          "name": "Rejected",
          "type": "_void",
          "description": "User or host rejected"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "SigningPayload",
      "name": "SigningPayload",
      "category": "Signing",
      "source": "sign.ts",
      "definition": "Struct({ account, blockHash, blockNumber, era, genesisHash, method, nonce, specVersion, tip, transactionVersion, signedExtensions, version, assetId?, metadataHash?, mode?, withSignedTransaction? })",
      "description": "Full Substrate extrinsic signing payload. V0.2: uses ProductAccountId instead of address string.",
      "fields": [
        {
          "name": "account",
          "type": "ProductAccountId",
          "description": "Product account that will sign this payload"
        },
        {
          "name": "blockHash",
          "type": "Hex()",
          "description": "Reference block hash"
        },
        {
          "name": "blockNumber",
          "type": "Hex()",
          "description": "Reference block number"
        },
        {
          "name": "era",
          "type": "Hex()",
          "description": "Mortality era encoding"
        },
        {
          "name": "genesisHash",
          "type": "GenesisHash",
          "description": "Chain genesis hash"
        },
        {
          "name": "method",
          "type": "Hex()",
          "description": "SCALE-encoded call data"
        },
        {
          "name": "nonce",
          "type": "Hex()",
          "description": "Account nonce"
        },
        {
          "name": "specVersion",
          "type": "Hex()",
          "description": "Runtime spec version"
        },
        {
          "name": "tip",
          "type": "Hex()",
          "description": "Transaction tip"
        },
        {
          "name": "transactionVersion",
          "type": "Hex()",
          "description": "Transaction format version"
        },
        {
          "name": "signedExtensions",
          "type": "Vector(str)",
          "description": "Extension identifiers"
        },
        {
          "name": "version",
          "type": "u32",
          "description": "Extrinsic version"
        },
        {
          "name": "assetId",
          "type": "Option(Hex())",
          "description": "For multi-asset tips"
        },
        {
          "name": "metadataHash",
          "type": "Option(Hex())",
          "description": "CheckMetadataHash extension"
        },
        {
          "name": "mode",
          "type": "Option(u32)",
          "description": "Metadata mode"
        },
        {
          "name": "withSignedTransaction",
          "type": "Option(bool)",
          "description": "Request signed tx back"
        }
      ],
      "variants": []
    },
    {
      "id": "RawPayload",
      "name": "RawPayload",
      "category": "Signing",
      "source": "sign.ts",
      "definition": "Enum({ Bytes: Bytes(), Payload: str })",
      "description": "Raw data to sign — either binary bytes or a string message.",
      "fields": [],
      "variants": [
        {
          "name": "Bytes",
          "type": "Bytes()",
          "description": "Raw binary data to sign"
        },
        {
          "name": "Payload",
          "type": "str",
          "description": "String message to sign"
        }
      ]
    },
    {
      "id": "SigningRawPayload",
      "name": "SigningRawPayload",
      "category": "Signing",
      "source": "sign.ts",
      "definition": "Struct({ account: ProductAccountId, payload: RawPayload })",
      "description": "A raw signing request pairing a product account with raw data. V0.2: uses ProductAccountId instead of address string.",
      "fields": [
        {
          "name": "account",
          "type": "ProductAccountId",
          "description": "Product account that will sign this data"
        },
        {
          "name": "payload",
          "type": "RawPayload",
          "description": "The data to sign"
        }
      ],
      "variants": []
    },
    {
      "id": "SigningPayloadPayload",
      "name": "SigningPayloadPayload",
      "category": "Signing",
      "source": "sign.ts",
      "definition": "Struct({ blockHash, blockNumber, era, genesisHash, method, nonce, specVersion, tip, transactionVersion, signedExtensions, version, assetId?, metadataHash?, mode?, withSignedTransaction? })",
      "description": "The structured payload fields of a SigningPayload, without the account.",
      "fields": [
        {
          "name": "blockHash",
          "type": "Hex()",
          "description": "Reference block hash"
        },
        {
          "name": "blockNumber",
          "type": "Hex()",
          "description": "Reference block number"
        },
        {
          "name": "era",
          "type": "Hex()",
          "description": "Mortality era encoding"
        },
        {
          "name": "genesisHash",
          "type": "GenesisHash",
          "description": "Chain genesis hash"
        },
        {
          "name": "method",
          "type": "Hex()",
          "description": "SCALE-encoded call data"
        },
        {
          "name": "nonce",
          "type": "Hex()",
          "description": "Account nonce"
        },
        {
          "name": "specVersion",
          "type": "Hex()",
          "description": "Runtime spec version"
        },
        {
          "name": "tip",
          "type": "Hex()",
          "description": "Transaction tip"
        },
        {
          "name": "transactionVersion",
          "type": "Hex()",
          "description": "Transaction format version"
        },
        {
          "name": "signedExtensions",
          "type": "Vector(str)",
          "description": "Extension identifiers"
        },
        {
          "name": "version",
          "type": "u32",
          "description": "Extrinsic version"
        },
        {
          "name": "assetId",
          "type": "Option(Hex())",
          "description": "For multi-asset tips"
        },
        {
          "name": "metadataHash",
          "type": "Option(Hex())",
          "description": "CheckMetadataHash extension"
        },
        {
          "name": "mode",
          "type": "Option(u32)",
          "description": "Metadata mode"
        },
        {
          "name": "withSignedTransaction",
          "type": "Option(bool)",
          "description": "Request signed tx back"
        }
      ],
      "variants": []
    },
    {
      "id": "SigningRawPayloadWithoutAccount",
      "name": "SigningRawPayloadWithoutAccount",
      "category": "Signing",
      "source": "sign.ts",
      "definition": "Struct({ signer: str, payload: RawPayload })",
      "description": "A raw signing request using a legacy (non-product) account.",
      "fields": [
        {
          "name": "signer",
          "type": "str",
          "description": "Signer identifier (e.g. SS58 address)"
        },
        {
          "name": "payload",
          "type": "RawPayload",
          "description": "The data to sign"
        }
      ],
      "variants": []
    },
    {
      "id": "SigningPayloadWithoutAccount",
      "name": "SigningPayloadWithoutAccount",
      "category": "Signing",
      "source": "sign.ts",
      "definition": "Struct({ signer: str, payload: SigningPayloadPayload })",
      "description": "A signing request using a legacy (non-product) account.",
      "fields": [
        {
          "name": "signer",
          "type": "str",
          "description": "Signer identifier (e.g. SS58 address)"
        },
        {
          "name": "payload",
          "type": "SigningPayloadPayload",
          "description": "The structured payload to sign"
        }
      ],
      "variants": []
    },
    {
      "id": "SigningResult",
      "name": "SigningResult",
      "category": "Signing",
      "source": "sign.ts",
      "definition": "Struct({ signature: Hex(), signedTransaction: Option(Hex()) })",
      "description": "Result of a signing operation.",
      "fields": [
        {
          "name": "signature",
          "type": "Hex()",
          "description": "The cryptographic signature"
        },
        {
          "name": "signedTransaction",
          "type": "Option(Hex())",
          "description": "Full signed transaction, if requested"
        }
      ],
      "variants": []
    },
    {
      "id": "SigningErr",
      "name": "SigningErr",
      "category": "Signing",
      "source": "sign.ts",
      "definition": "ErrEnum { FailedToDecode, Rejected, PermissionDenied, Unknown({ reason: str }) }",
      "description": "Signing operation error.",
      "fields": [],
      "variants": [
        {
          "name": "FailedToDecode",
          "type": "_void",
          "description": "Payload could not be deserialized"
        },
        {
          "name": "Rejected",
          "type": "_void",
          "description": "User rejected signing"
        },
        {
          "name": "PermissionDenied",
          "type": "_void",
          "description": "Not authenticated"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "TxPayloadExtensionV1",
      "name": "TxPayloadExtensionV1",
      "category": "Transaction",
      "source": "createTransaction.ts",
      "definition": "Struct({ id: str, extra: Hex(), additionalSigned: Hex() })",
      "description": "A signed extension for a transaction payload.",
      "fields": [
        {
          "name": "id",
          "type": "str",
          "description": "Extension name (e.g., \"CheckSpecVersion\")"
        },
        {
          "name": "extra",
          "type": "Hex()",
          "description": "SCALE-encoded extra data (in extrinsic body)"
        },
        {
          "name": "additionalSigned",
          "type": "Hex()",
          "description": "SCALE-encoded implicit data (signed, not in body)"
        }
      ],
      "variants": []
    },
    {
      "id": "TxPayloadContextV1",
      "name": "TxPayloadContextV1",
      "category": "Transaction",
      "source": "createTransaction.ts",
      "definition": "Struct({ metadata: Hex(), tokenSymbol: str, tokenDecimals: u32, bestBlockHeight: u32 })",
      "description": "Context information for transaction construction.",
      "fields": [
        {
          "name": "metadata",
          "type": "Hex()",
          "description": "RuntimeMetadataPrefixed blob (SCALE)"
        },
        {
          "name": "tokenSymbol",
          "type": "str",
          "description": "Native token symbol"
        },
        {
          "name": "tokenDecimals",
          "type": "u32",
          "description": "Native token decimals"
        },
        {
          "name": "bestBlockHeight",
          "type": "u32",
          "description": "Highest known block number"
        }
      ],
      "variants": []
    },
    {
      "id": "TxPayloadV1",
      "name": "TxPayloadV1",
      "category": "Transaction",
      "source": "createTransaction.ts",
      "definition": "Struct({ signer: Nullable(str), callData: Hex(), extensions: Vector(TxPayloadExtensionV1), txExtVersion: u8, context: TxPayloadContextV1 })",
      "description": "Version 1 transaction payload with all data needed to construct a signed extrinsic.",
      "fields": [
        {
          "name": "signer",
          "type": "Nullable(str)",
          "description": "Signer hint (address/name), null = host picks"
        },
        {
          "name": "callData",
          "type": "Hex()",
          "description": "SCALE-encoded Call data"
        },
        {
          "name": "extensions",
          "type": "Vector(TxPayloadExtensionV1)",
          "description": "Signed extensions"
        },
        {
          "name": "txExtVersion",
          "type": "u8",
          "description": "0 for Extrinsic V4, any for V5"
        },
        {
          "name": "context",
          "type": "TxPayloadContextV1",
          "description": "Transaction context"
        }
      ],
      "variants": []
    },
    {
      "id": "VersionedTxPayload",
      "name": "VersionedTxPayload",
      "category": "Transaction",
      "source": "createTransaction.ts",
      "definition": "Enum({ v1: TxPayloadV1 })",
      "description": "Versioned transaction payload envelope.",
      "fields": [],
      "variants": [
        {
          "name": "v1",
          "type": "TxPayloadV1",
          "description": "Version 1 payload"
        }
      ]
    },
    {
      "id": "CreateTransactionErr",
      "name": "CreateTransactionErr",
      "category": "Transaction",
      "source": "createTransaction.ts",
      "definition": "ErrEnum { FailedToDecode, Rejected, NotSupported(str), PermissionDenied, Unknown({ reason: str }) }",
      "description": "Transaction creation error.",
      "fields": [],
      "variants": [
        {
          "name": "FailedToDecode",
          "type": "_void",
          "description": "Payload could not be deserialized"
        },
        {
          "name": "Rejected",
          "type": "_void",
          "description": "User rejected"
        },
        {
          "name": "NotSupported",
          "type": "str",
          "description": "Unsupported payload version or extension"
        },
        {
          "name": "PermissionDenied",
          "type": "_void",
          "description": "Not authenticated"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "StorageKey",
      "name": "StorageKey",
      "category": "Storage",
      "source": "localStorage.ts",
      "definition": "str",
      "description": "Key name for local storage operations.",
      "fields": [],
      "variants": []
    },
    {
      "id": "StorageValue",
      "name": "StorageValue",
      "category": "Storage",
      "source": "localStorage.ts",
      "definition": "Bytes()",
      "description": "Binary value stored in local storage.",
      "fields": [],
      "variants": []
    },
    {
      "id": "StorageErr",
      "name": "StorageErr",
      "category": "Storage",
      "source": "localStorage.ts",
      "definition": "ErrEnum { Full, Unknown({ reason: str }) }",
      "description": "Local storage operation error.",
      "fields": [],
      "variants": [
        {
          "name": "Full",
          "type": "_void",
          "description": "Storage quota exceeded"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "NavigateToErr",
      "name": "NavigateToErr",
      "category": "Navigation",
      "source": "navigation.ts",
      "definition": "ErrEnum { PermissionDenied, Unknown({ reason: str }) }",
      "description": "Navigation error.",
      "fields": [],
      "variants": [
        {
          "name": "PermissionDenied",
          "type": "_void",
          "description": "Navigation not allowed"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "PushNotification",
      "name": "PushNotification",
      "category": "Notification",
      "source": "notification.ts",
      "definition": "Struct({ text: str, deeplink: Option(str) })",
      "description": "Push notification payload.",
      "fields": [
        {
          "name": "text",
          "type": "str",
          "description": "Notification text"
        },
        {
          "name": "deeplink",
          "type": "Option(str)",
          "description": "Optional URL to open on tap"
        }
      ],
      "variants": []
    },
    {
      "id": "DevicePermission",
      "name": "DevicePermission",
      "category": "Permission",
      "source": "devicePermission.ts",
      "definition": "Status('Notifications', 'Camera', 'Microphone', 'Bluetooth', 'Nfc', 'Location', 'Clipboard', 'OpenUrl', 'Biometrics')",
      "description": "Device capability to request access to. V0.2: extended to 9 variants including Notifications, Nfc, Clipboard, OpenUrl, and Biometrics.",
      "fields": [],
      "variants": []
    },
    {
      "id": "RemotePermission",
      "name": "RemotePermission",
      "category": "Permission",
      "source": "remotePermission.ts",
      "definition": "Enum({ Remote: Vector(str), WebRtc: _void, ChainSubmit: _void, StatementSubmit: _void, PreimageSubmit: _void })",
      "description": "A single remote-operation permission entry. V0.2: replaces RemotePermissionRequest with batching support via Vector(RemotePermission).",
      "fields": [],
      "variants": [
        {
          "name": "Remote",
          "type": "Vector(str)",
          "description": "HTTP/HTTPS/WS/WSS access to specific domain patterns"
        },
        {
          "name": "WebRtc",
          "type": "_void",
          "description": "WebRTC access (can expose user IP)"
        },
        {
          "name": "ChainSubmit",
          "type": "_void",
          "description": "Broadcast signed transactions via remote_chain_transaction_broadcast"
        },
        {
          "name": "StatementSubmit",
          "type": "_void",
          "description": "Submit statements via remote_statement_store_submit"
        },
        {
          "name": "PreimageSubmit",
          "type": "_void",
          "description": "Submit preimages via remote_preimage_submit"
        }
      ]
    },
    {
      "id": "Feature",
      "name": "Feature",
      "category": "Feature",
      "source": "feature.ts",
      "definition": "Enum({ Chain: GenesisHash })",
      "description": "Feature to check for host support.",
      "fields": [],
      "variants": [
        {
          "name": "Chain",
          "type": "GenesisHash",
          "description": "Is this blockchain supported?"
        }
      ]
    },
    {
      "id": "ChatRoomRequest",
      "name": "ChatRoomRequest",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ roomId: str, name: str, icon: str })",
      "description": "Request to create a chat room.",
      "fields": [
        {
          "name": "roomId",
          "type": "str",
          "description": "Unique room identifier"
        },
        {
          "name": "name",
          "type": "str",
          "description": "Room display name"
        },
        {
          "name": "icon",
          "type": "str",
          "description": "URL or base64 image"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatRoomRegistrationStatus",
      "name": "ChatRoomRegistrationStatus",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Status('New', 'Exists')",
      "description": "Whether the room was newly created or already existed.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ChatRoomRegistrationResult",
      "name": "ChatRoomRegistrationResult",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ status: ChatRoomRegistrationStatus })",
      "description": "Result of a room registration.",
      "fields": [
        {
          "name": "status",
          "type": "ChatRoomRegistrationStatus",
          "description": "\"New\" or \"Exists\""
        }
      ],
      "variants": []
    },
    {
      "id": "SimpleGroupChatRequest",
      "name": "SimpleGroupChatRequest",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ roomId: str, name: str, icon: str })",
      "description": "Request to create a simple group chat room. V0.2 addition: lightweight group chat where participants join via deep link.",
      "fields": [
        {
          "name": "roomId",
          "type": "str",
          "description": "Unique room identifier source"
        },
        {
          "name": "name",
          "type": "str",
          "description": "Room display name"
        },
        {
          "name": "icon",
          "type": "str",
          "description": "URL or base64 image for room avatar"
        }
      ],
      "variants": []
    },
    {
      "id": "SimpleGroupChatResult",
      "name": "SimpleGroupChatResult",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ status: ChatRoomRegistrationStatus, joinLink: str })",
      "description": "Result of creating a simple group chat room. V0.2 addition.",
      "fields": [
        {
          "name": "status",
          "type": "ChatRoomRegistrationStatus",
          "description": "Whether the room was newly created or already existed"
        },
        {
          "name": "joinLink",
          "type": "str",
          "description": "Deep link that participants can use to join the room"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatBotRequest",
      "name": "ChatBotRequest",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ botId: str, name: str, icon: str })",
      "description": "Request to register a chat bot.",
      "fields": [
        {
          "name": "botId",
          "type": "str",
          "description": "Unique bot identifier"
        },
        {
          "name": "name",
          "type": "str",
          "description": "Bot display name"
        },
        {
          "name": "icon",
          "type": "str",
          "description": "URL or base64 image"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatBotRegistrationStatus",
      "name": "ChatBotRegistrationStatus",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Status('New', 'Exists')",
      "description": "Whether the bot was newly registered or already existed.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ChatBotRegistrationResult",
      "name": "ChatBotRegistrationResult",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ status: ChatBotRegistrationStatus })",
      "description": "Result of a bot registration.",
      "fields": [
        {
          "name": "status",
          "type": "ChatBotRegistrationStatus",
          "description": "\"New\" or \"Exists\""
        }
      ],
      "variants": []
    },
    {
      "id": "ChatRoomParticipation",
      "name": "ChatRoomParticipation",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Status('RoomHost', 'Bot')",
      "description": "How the product participates in a chat room.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ChatRoom",
      "name": "ChatRoom",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ roomId: str, participatingAs: ChatRoomParticipation })",
      "description": "A chat room the product participates in.",
      "fields": [
        {
          "name": "roomId",
          "type": "str",
          "description": "Room identifier"
        },
        {
          "name": "participatingAs",
          "type": "ChatRoomParticipation",
          "description": "\"RoomHost\" or \"Bot\""
        }
      ],
      "variants": []
    },
    {
      "id": "ChatAction",
      "name": "ChatAction",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ actionId: str, title: str })",
      "description": "A clickable action button in a chat message.",
      "fields": [
        {
          "name": "actionId",
          "type": "str",
          "description": "Action identifier"
        },
        {
          "name": "title",
          "type": "str",
          "description": "Button label"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatActionLayout",
      "name": "ChatActionLayout",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Status('Column', 'Grid')",
      "description": "Layout for action buttons.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ChatActions",
      "name": "ChatActions",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ text: Option(str), actions: Vector(ChatAction), layout: ChatActionLayout })",
      "description": "A set of action buttons with optional text.",
      "fields": [
        {
          "name": "text",
          "type": "Option(str)",
          "description": "Optional message text"
        },
        {
          "name": "actions",
          "type": "Vector(ChatAction)",
          "description": "List of action buttons"
        },
        {
          "name": "layout",
          "type": "ChatActionLayout",
          "description": "\"Column\" or \"Grid\" layout"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatMedia",
      "name": "ChatMedia",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ url: str })",
      "description": "A media attachment.",
      "fields": [
        {
          "name": "url",
          "type": "str",
          "description": "Media URL"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatRichText",
      "name": "ChatRichText",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ text: Option(str), media: Vector(ChatMedia) })",
      "description": "Rich text message with optional media.",
      "fields": [
        {
          "name": "text",
          "type": "Option(str)",
          "description": "Optional text content"
        },
        {
          "name": "media",
          "type": "Vector(ChatMedia)",
          "description": "Attached media items"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatFile",
      "name": "ChatFile",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ url: str, fileName: str, mimeType: str, sizeBytes: u64, text: Option(str) })",
      "description": "A file attachment in a chat message.",
      "fields": [
        {
          "name": "url",
          "type": "str",
          "description": "File download URL"
        },
        {
          "name": "fileName",
          "type": "str",
          "description": "File name"
        },
        {
          "name": "mimeType",
          "type": "str",
          "description": "MIME type"
        },
        {
          "name": "sizeBytes",
          "type": "u64",
          "description": "File size in bytes"
        },
        {
          "name": "text",
          "type": "Option(str)",
          "description": "Optional caption text"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatReaction",
      "name": "ChatReaction",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ messageId: str, emoji: str })",
      "description": "A reaction to a chat message.",
      "fields": [
        {
          "name": "messageId",
          "type": "str",
          "description": "Message being reacted to"
        },
        {
          "name": "emoji",
          "type": "str",
          "description": "Emoji reaction"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatCustomMessage",
      "name": "ChatCustomMessage",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ messageType: str, payload: Bytes() })",
      "description": "A custom message with application-defined type and binary payload.",
      "fields": [
        {
          "name": "messageType",
          "type": "str",
          "description": "Application-defined type key"
        },
        {
          "name": "payload",
          "type": "Bytes()",
          "description": "Binary payload"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatMessageContent",
      "name": "ChatMessageContent",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Enum({ Text: str, RichText: ChatRichText, Actions: ChatActions, File: ChatFile, Reaction: ChatReaction, ReactionRemoved: ChatReaction, Custom: ChatCustomMessage })",
      "description": "Content of a chat message — one of several types.",
      "fields": [],
      "variants": [
        {
          "name": "Text",
          "type": "str",
          "description": "Plain text message"
        },
        {
          "name": "RichText",
          "type": "ChatRichText",
          "description": "Rich text with media"
        },
        {
          "name": "Actions",
          "type": "ChatActions",
          "description": "Action button set"
        },
        {
          "name": "File",
          "type": "ChatFile",
          "description": "File attachment"
        },
        {
          "name": "Reaction",
          "type": "ChatReaction",
          "description": "Emoji reaction"
        },
        {
          "name": "ReactionRemoved",
          "type": "ChatReaction",
          "description": "Reaction removal"
        },
        {
          "name": "Custom",
          "type": "ChatCustomMessage",
          "description": "Custom message"
        }
      ]
    },
    {
      "id": "ChatPostMessageResult",
      "name": "ChatPostMessageResult",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ messageId: str })",
      "description": "Result of posting a message.",
      "fields": [
        {
          "name": "messageId",
          "type": "str",
          "description": "Assigned message ID"
        }
      ],
      "variants": []
    },
    {
      "id": "ActionTrigger",
      "name": "ActionTrigger",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ messageId: str, actionId: str, payload: Option(Bytes()) })",
      "description": "Payload when a user clicks an action button.",
      "fields": [
        {
          "name": "messageId",
          "type": "str",
          "description": "Message containing the action"
        },
        {
          "name": "actionId",
          "type": "str",
          "description": "Which action was triggered"
        },
        {
          "name": "payload",
          "type": "Option(Bytes())",
          "description": "Optional additional data"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatCommand",
      "name": "ChatCommand",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ command: str, payload: str })",
      "description": "A slash command from a chat user.",
      "fields": [
        {
          "name": "command",
          "type": "str",
          "description": "Command name"
        },
        {
          "name": "payload",
          "type": "str",
          "description": "Command arguments"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatActionPayload",
      "name": "ChatActionPayload",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Enum({ MessagePosted: ChatMessageContent, ActionTriggered: ActionTrigger, Command: ChatCommand })",
      "description": "Payload of a received chat action.",
      "fields": [],
      "variants": [
        {
          "name": "MessagePosted",
          "type": "ChatMessageContent",
          "description": "A peer posted a message"
        },
        {
          "name": "ActionTriggered",
          "type": "ActionTrigger",
          "description": "A user triggered an action button"
        },
        {
          "name": "Command",
          "type": "ChatCommand",
          "description": "A user issued a command"
        }
      ]
    },
    {
      "id": "ReceivedChatAction",
      "name": "ReceivedChatAction",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "Struct({ roomId: str, peer: str, payload: ChatActionPayload })",
      "description": "A chat action received from the host.",
      "fields": [
        {
          "name": "roomId",
          "type": "str",
          "description": "Room where the action occurred"
        },
        {
          "name": "peer",
          "type": "str",
          "description": "Peer who initiated the action"
        },
        {
          "name": "payload",
          "type": "ChatActionPayload",
          "description": "The action payload"
        }
      ],
      "variants": []
    },
    {
      "id": "ChatRoomRegistrationErr",
      "name": "ChatRoomRegistrationErr",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "ErrEnum { PermissionDenied, Unknown({ reason: str }) }",
      "description": "Chat room registration error.",
      "fields": [],
      "variants": [
        {
          "name": "PermissionDenied",
          "type": "_void",
          "description": "Not allowed"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "ChatBotRegistrationErr",
      "name": "ChatBotRegistrationErr",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "ErrEnum { PermissionDenied, Unknown({ reason: str }) }",
      "description": "Chat bot registration error.",
      "fields": [],
      "variants": [
        {
          "name": "PermissionDenied",
          "type": "_void",
          "description": "Not allowed"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "ChatMessagePostingErr",
      "name": "ChatMessagePostingErr",
      "category": "Chat",
      "source": "chat.ts",
      "definition": "ErrEnum { MessageTooLarge, Unknown({ reason: str }) }",
      "description": "Chat message posting error.",
      "fields": [],
      "variants": [
        {
          "name": "MessageTooLarge",
          "type": "_void",
          "description": "Message exceeded size limit"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "Size",
      "name": "Size",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "compact",
      "description": "Variable-length unsigned integer used for dimensions.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Dimensions",
      "name": "Dimensions",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Tuple(Size, Size, Option(Size), Option(Size))",
      "description": "CSS-like dimensions: (top, end, bottom?, start?). Bottom defaults to top, start defaults to end.",
      "fields": [],
      "variants": []
    },
    {
      "id": "TypographyStyle",
      "name": "TypographyStyle",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Status('titleXL', 'headline', 'bodyM', 'bodyS', 'caption')",
      "description": "Text typography presets.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ButtonVariant",
      "name": "ButtonVariant",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Status('primary', 'secondary', 'text')",
      "description": "Button style variants.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ColorToken",
      "name": "ColorToken",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Status('textPrimary', 'textSecondary', 'textTertiary', 'backgroundPrimary', 'backgroundSecondary', 'backgroundTertiary', 'success', 'error', 'warning')",
      "description": "Semantic color tokens for theming.",
      "fields": [],
      "variants": []
    },
    {
      "id": "ContentAlignment",
      "name": "ContentAlignment",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Status('topStart', 'topCenter', 'topEnd', 'centerStart', 'center', 'centerEnd', 'bottomStart', 'bottomCenter', 'bottomEnd')",
      "description": "2D content alignment.",
      "fields": [],
      "variants": []
    },
    {
      "id": "HorizontalAlignment",
      "name": "HorizontalAlignment",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Status('start', 'center', 'end')",
      "description": "Horizontal alignment options.",
      "fields": [],
      "variants": []
    },
    {
      "id": "VerticalAlignment",
      "name": "VerticalAlignment",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Status('top', 'center', 'bottom')",
      "description": "Vertical alignment options.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Arrangement",
      "name": "Arrangement",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Status('start', 'end', 'center', 'spaceBetween', 'spaceAround', 'spaceEvenly')",
      "description": "Layout arrangement (like CSS flexbox justify-content).",
      "fields": [],
      "variants": []
    },
    {
      "id": "Shape",
      "name": "Shape",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Enum({ Rounded: Size, Circle: _void })",
      "description": "Shape for borders and backgrounds.",
      "fields": [],
      "variants": [
        {
          "name": "Rounded",
          "type": "Size",
          "description": "Border radius value"
        },
        {
          "name": "Circle",
          "type": "_void",
          "description": "Circular shape"
        }
      ]
    },
    {
      "id": "BorderStyle",
      "name": "BorderStyle",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Struct({ width: Size, color: ColorToken, shape: Option(Shape) })",
      "description": "Border styling.",
      "fields": [
        {
          "name": "width",
          "type": "Size",
          "description": "Border width"
        },
        {
          "name": "color",
          "type": "ColorToken",
          "description": "Border color"
        },
        {
          "name": "shape",
          "type": "Option(Shape)",
          "description": "Border shape"
        }
      ],
      "variants": []
    },
    {
      "id": "Modifier",
      "name": "Modifier",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Enum({ margin, padding, background, border, height, width, minWidth, minHeight, fillWidth, fillHeight })",
      "description": "Layout and styling modifiers applied to custom renderer components.",
      "fields": [],
      "variants": [
        {
          "name": "margin",
          "type": "Dimensions",
          "description": "Outer spacing"
        },
        {
          "name": "padding",
          "type": "Dimensions",
          "description": "Inner spacing"
        },
        {
          "name": "background",
          "type": "Struct({ color: ColorToken, shape: Option(Shape) })",
          "description": "Background fill"
        },
        {
          "name": "border",
          "type": "BorderStyle",
          "description": "Border style"
        },
        {
          "name": "height",
          "type": "Size",
          "description": "Fixed height"
        },
        {
          "name": "width",
          "type": "Size",
          "description": "Fixed width"
        },
        {
          "name": "minWidth",
          "type": "Size",
          "description": "Minimum width"
        },
        {
          "name": "minHeight",
          "type": "Size",
          "description": "Minimum height"
        },
        {
          "name": "fillWidth",
          "type": "bool",
          "description": "Fill available width"
        },
        {
          "name": "fillHeight",
          "type": "bool",
          "description": "Fill available height"
        }
      ]
    },
    {
      "id": "CustomRendererNode",
      "name": "CustomRendererNode",
      "category": "Custom Renderer",
      "source": "customRenderer.ts",
      "definition": "Enum({ Nil: _void, String: str, Box: Component<BoxProps>, Column: Component<ColumnProps>, Row: Component<RowProps>, Spacer: Component<_void>, Text: Component<TextProps>, Button: Component<ButtonProps>, TextField: Component<TextFieldProps> })",
      "description": "A node in the custom renderer UI tree. Can be nested recursively via children.",
      "fields": [],
      "variants": [
        {
          "name": "Nil",
          "type": "_void",
          "description": "Empty node"
        },
        {
          "name": "String",
          "type": "str",
          "description": "Raw text string"
        },
        {
          "name": "Box",
          "type": "Component<BoxProps>",
          "description": "Generic container"
        },
        {
          "name": "Column",
          "type": "Component<ColumnProps>",
          "description": "Vertical layout"
        },
        {
          "name": "Row",
          "type": "Component<RowProps>",
          "description": "Horizontal layout"
        },
        {
          "name": "Spacer",
          "type": "Component<_void>",
          "description": "Flexible space"
        },
        {
          "name": "Text",
          "type": "Component<TextProps>",
          "description": "Text display"
        },
        {
          "name": "Button",
          "type": "Component<ButtonProps>",
          "description": "Interactive button"
        },
        {
          "name": "TextField",
          "type": "Component<TextFieldProps>",
          "description": "Text input"
        }
      ]
    },
    {
      "id": "BlockHash",
      "name": "BlockHash",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Hex()",
      "description": "Block hash identifier.",
      "fields": [],
      "variants": []
    },
    {
      "id": "OperationId",
      "name": "OperationId",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "str",
      "description": "Operation identifier for async chain operations.",
      "fields": [],
      "variants": []
    },
    {
      "id": "RuntimeApi",
      "name": "RuntimeApi",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Tuple(str, u32)",
      "description": "A runtime API identified by name and version.",
      "fields": [],
      "variants": []
    },
    {
      "id": "RuntimeSpec",
      "name": "RuntimeSpec",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Struct({ specName, implName, specVersion, implVersion, transactionVersion?, apis })",
      "description": "Runtime specification metadata.",
      "fields": [
        {
          "name": "specName",
          "type": "str",
          "description": "Specification name"
        },
        {
          "name": "implName",
          "type": "str",
          "description": "Implementation name"
        },
        {
          "name": "specVersion",
          "type": "u32",
          "description": "Spec version number"
        },
        {
          "name": "implVersion",
          "type": "u32",
          "description": "Implementation version"
        },
        {
          "name": "transactionVersion",
          "type": "Option(u32)",
          "description": "Transaction format version"
        },
        {
          "name": "apis",
          "type": "Vector(RuntimeApi)",
          "description": "Supported runtime APIs"
        }
      ],
      "variants": []
    },
    {
      "id": "RuntimeType",
      "name": "RuntimeType",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Enum({ Valid: RuntimeSpec, Invalid: Struct({ error: str }) })",
      "description": "Runtime validity check result.",
      "fields": [],
      "variants": [
        {
          "name": "Valid",
          "type": "RuntimeSpec",
          "description": "Valid runtime with spec"
        },
        {
          "name": "Invalid",
          "type": "Struct({ error: str })",
          "description": "Invalid runtime with error"
        }
      ]
    },
    {
      "id": "StorageQueryType",
      "name": "StorageQueryType",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Status('Value', 'Hash', 'ClosestDescendantMerkleValue', 'DescendantsValues', 'DescendantsHashes')",
      "description": "Type of storage query to perform.",
      "fields": [],
      "variants": []
    },
    {
      "id": "StorageQueryItem",
      "name": "StorageQueryItem",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Struct({ key: Hex(), type: StorageQueryType })",
      "description": "A single storage query.",
      "fields": [
        {
          "name": "key",
          "type": "Hex()",
          "description": "Storage key to query"
        },
        {
          "name": "type",
          "type": "StorageQueryType",
          "description": "What to return"
        }
      ],
      "variants": []
    },
    {
      "id": "StorageResultItem",
      "name": "StorageResultItem",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Struct({ key, value, hash, closestDescendantMerkleValue })",
      "description": "Result of a storage query.",
      "fields": [
        {
          "name": "key",
          "type": "Hex()",
          "description": "The queried key"
        },
        {
          "name": "value",
          "type": "Nullable(Hex())",
          "description": "Value, if requested"
        },
        {
          "name": "hash",
          "type": "Nullable(Hex())",
          "description": "Hash, if requested"
        },
        {
          "name": "closestDescendantMerkleValue",
          "type": "Nullable(Hex())",
          "description": "Merkle value, if requested"
        }
      ],
      "variants": []
    },
    {
      "id": "OperationStartedResult",
      "name": "OperationStartedResult",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Enum({ Started: Struct({ operationId: OperationId }), LimitReached: _void })",
      "description": "Result of starting a chain operation.",
      "fields": [],
      "variants": [
        {
          "name": "Started",
          "type": "Struct({ operationId: OperationId })",
          "description": "Operation started successfully"
        },
        {
          "name": "LimitReached",
          "type": "_void",
          "description": "Too many concurrent operations"
        }
      ]
    },
    {
      "id": "ChainHeadEvent",
      "name": "ChainHeadEvent",
      "category": "Chain",
      "source": "chainInteraction.ts",
      "definition": "Enum with 12 variants",
      "description": "Events received when following the chain head.",
      "fields": [],
      "variants": [
        {
          "name": "Initialized",
          "type": "Struct({ finalizedBlockHashes, finalizedBlockRuntime? })",
          "description": "Initial state with finalized blocks"
        },
        {
          "name": "NewBlock",
          "type": "Struct({ blockHash, parentBlockHash, newRuntime? })",
          "description": "A new block was produced"
        },
        {
          "name": "BestBlockChanged",
          "type": "Struct({ bestBlockHash })",
          "description": "Best block changed"
        },
        {
          "name": "Finalized",
          "type": "Struct({ finalizedBlockHashes, prunedBlockHashes })",
          "description": "Blocks were finalized"
        },
        {
          "name": "OperationBodyDone",
          "type": "Struct({ operationId, value })",
          "description": "Body fetch completed"
        },
        {
          "name": "OperationCallDone",
          "type": "Struct({ operationId, output })",
          "description": "Runtime call completed"
        },
        {
          "name": "OperationStorageItems",
          "type": "Struct({ operationId, items })",
          "description": "Storage results batch"
        },
        {
          "name": "OperationStorageDone",
          "type": "Struct({ operationId })",
          "description": "Storage query completed"
        },
        {
          "name": "OperationWaitingForContinue",
          "type": "Struct({ operationId })",
          "description": "Operation paused, needs continue"
        },
        {
          "name": "OperationInaccessible",
          "type": "Struct({ operationId })",
          "description": "Block became inaccessible"
        },
        {
          "name": "OperationError",
          "type": "Struct({ operationId, error })",
          "description": "Operation failed"
        },
        {
          "name": "Stop",
          "type": "_void",
          "description": "Subscription terminated by server"
        }
      ]
    },
    {
      "id": "Topic",
      "name": "Topic",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "Bytes(32)",
      "description": "32-byte topic identifier.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Channel",
      "name": "Channel",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "Bytes(32)",
      "description": "32-byte channel identifier.",
      "fields": [],
      "variants": []
    },
    {
      "id": "DecryptionKey",
      "name": "DecryptionKey",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "Bytes(32)",
      "description": "32-byte decryption key.",
      "fields": [],
      "variants": []
    },
    {
      "id": "TopicFilter",
      "name": "TopicFilter",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "Enum({ MatchAll: Vector(Topic), MatchAny: Vector(Topic) })",
      "description": "Filter for statement subscriptions. MatchAll requires every listed topic (AND). MatchAny requires at least one listed topic (OR). V0.2 addition replacing plain topic vectors.",
      "fields": [],
      "variants": [
        {
          "name": "MatchAll",
          "type": "Vector(Topic)",
          "description": "AND: statement must contain every listed topic."
        },
        {
          "name": "MatchAny",
          "type": "Vector(Topic)",
          "description": "OR: statement must contain at least one listed topic."
        }
      ]
    },
    {
      "id": "StatementProof",
      "name": "StatementProof",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "Enum({ Sr25519, Ed25519, Ecdsa, OnChain })",
      "description": "Cryptographic proof for a statement.",
      "fields": [],
      "variants": [
        {
          "name": "Sr25519",
          "type": "Struct({ signature: Bytes(64), signer: Bytes(32) })",
          "description": "Sr25519 signature proof"
        },
        {
          "name": "Ed25519",
          "type": "Struct({ signature: Bytes(64), signer: Bytes(32) })",
          "description": "Ed25519 signature proof"
        },
        {
          "name": "Ecdsa",
          "type": "Struct({ signature: Bytes(65), signer: Bytes(33) })",
          "description": "ECDSA signature proof"
        },
        {
          "name": "OnChain",
          "type": "Struct({ who: Bytes(32), blockHash: Bytes(32), event: u64 })",
          "description": "On-chain event proof"
        }
      ]
    },
    {
      "id": "Statement",
      "name": "Statement",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "Struct({ proof?, decryptionKey?, expiry?, channel?, topics, data? })",
      "description": "A statement with optional proof and metadata.",
      "fields": [
        {
          "name": "proof",
          "type": "Option(StatementProof)",
          "description": "Optional cryptographic proof"
        },
        {
          "name": "decryptionKey",
          "type": "Option(DecryptionKey)",
          "description": "Optional decryption key"
        },
        {
          "name": "expiry",
          "type": "Option(u64)",
          "description": "Optional Unix timestamp expiry"
        },
        {
          "name": "channel",
          "type": "Option(Channel)",
          "description": "Optional channel"
        },
        {
          "name": "topics",
          "type": "Vector(Topic)",
          "description": "Topic tags"
        },
        {
          "name": "data",
          "type": "Option(Bytes())",
          "description": "Optional data payload"
        }
      ],
      "variants": []
    },
    {
      "id": "SignedStatement",
      "name": "SignedStatement",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "Struct({ proof, decryptionKey?, expiry?, channel?, topics, data? })",
      "description": "A statement with required (not optional) proof.",
      "fields": [
        {
          "name": "proof",
          "type": "StatementProof",
          "description": "Required cryptographic proof"
        },
        {
          "name": "decryptionKey",
          "type": "Option(DecryptionKey)",
          "description": "Optional decryption key"
        },
        {
          "name": "expiry",
          "type": "Option(u64)",
          "description": "Optional Unix timestamp expiry"
        },
        {
          "name": "channel",
          "type": "Option(Channel)",
          "description": "Optional channel"
        },
        {
          "name": "topics",
          "type": "Vector(Topic)",
          "description": "Topic tags"
        },
        {
          "name": "data",
          "type": "Option(Bytes())",
          "description": "Optional data payload"
        }
      ],
      "variants": []
    },
    {
      "id": "SignedStatementsPage",
      "name": "SignedStatementsPage",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "Struct({ statements: Vector(SignedStatement), isComplete: bool })",
      "description": "A page of signed statements delivered to subscribers.",
      "fields": [
        {
          "name": "statements",
          "type": "Vector(SignedStatement)",
          "description": "Statements in this page"
        },
        {
          "name": "isComplete",
          "type": "bool",
          "description": "false = intermediate page of initial dump; true = initial dump complete"
        }
      ],
      "variants": []
    },
    {
      "id": "StatementProofErr",
      "name": "StatementProofErr",
      "category": "Statement Store",
      "source": "statementStore.ts",
      "definition": "ErrEnum { UnableToSign, UnknownAccount, Unknown({ reason: str }) }",
      "description": "Statement proof creation error.",
      "fields": [],
      "variants": [
        {
          "name": "UnableToSign",
          "type": "_void",
          "description": "Signing operation failed"
        },
        {
          "name": "UnknownAccount",
          "type": "_void",
          "description": "Account not recognized"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "PreimageKey",
      "name": "PreimageKey",
      "category": "Preimage",
      "source": "preimage.ts",
      "definition": "Hex()",
      "description": "Hash of the preimage.",
      "fields": [],
      "variants": []
    },
    {
      "id": "PreimageValue",
      "name": "PreimageValue",
      "category": "Preimage",
      "source": "preimage.ts",
      "definition": "Bytes()",
      "description": "The preimage data.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Balance",
      "name": "Balance",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "u128",
      "description": "Balance amount for payment operations. Interpreted according to the host's single fixed payment asset (e.g. pUSD).",
      "fields": [],
      "variants": []
    },
    {
      "id": "PaymentId",
      "name": "PaymentId",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "str",
      "description": "Unique payment identifier, scoped to the product that created it.",
      "fields": [],
      "variants": []
    },
    {
      "id": "Ed25519PrivateKey",
      "name": "Ed25519PrivateKey",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "Bytes(32)",
      "description": "Ed25519 private key bytes (32 bytes).",
      "fields": [],
      "variants": []
    },
    {
      "id": "PaymentBalance",
      "name": "PaymentBalance",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "Struct({ available: Balance })",
      "description": "Current payment balance state pushed to subscribers.",
      "fields": [
        {
          "name": "available",
          "type": "Balance",
          "description": "Balance that can be spent right now"
        }
      ],
      "variants": []
    },
    {
      "id": "PaymentTopUpSource",
      "name": "PaymentTopUpSource",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "Enum({ ProductAccount: DerivationIndex, PrivateKey: Ed25519PrivateKey })",
      "description": "Source for a payment top-up operation.",
      "fields": [],
      "variants": [
        {
          "name": "ProductAccount",
          "type": "DerivationIndex",
          "description": "Fund from one of the calling product's scoped accounts"
        },
        {
          "name": "PrivateKey",
          "type": "Ed25519PrivateKey",
          "description": "Fund from a one-time account represented by its private key"
        }
      ]
    },
    {
      "id": "PaymentReceipt",
      "name": "PaymentReceipt",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "Struct({ id: PaymentId })",
      "description": "Receipt returned after a successful payment request.",
      "fields": [
        {
          "name": "id",
          "type": "PaymentId",
          "description": "The assigned payment identifier"
        }
      ],
      "variants": []
    },
    {
      "id": "PaymentStatus",
      "name": "PaymentStatus",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "Enum({ Processing: _void, Completed: _void, Failed: str })",
      "description": "Payment lifecycle status. Once a terminal state (Completed or Failed) is reached, the host delivers it and may close the subscription.",
      "fields": [],
      "variants": [
        {
          "name": "Processing",
          "type": "_void",
          "description": "Payment is being processed"
        },
        {
          "name": "Completed",
          "type": "_void",
          "description": "Payment has been settled successfully"
        },
        {
          "name": "Failed",
          "type": "str",
          "description": "Payment has failed with a reason"
        }
      ]
    },
    {
      "id": "PaymentBalanceErr",
      "name": "PaymentBalanceErr",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "ErrEnum { PermissionDenied, Unknown({ reason: str }) }",
      "description": "Error from host_payment_balance_subscribe.",
      "fields": [],
      "variants": [
        {
          "name": "PermissionDenied",
          "type": "_void",
          "description": "User denied the balance disclosure request"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "PaymentTopUpErr",
      "name": "PaymentTopUpErr",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "ErrEnum { InsufficientFunds, InvalidSource, Unknown({ reason: str }) }",
      "description": "Error from host_payment_top_up.",
      "fields": [],
      "variants": [
        {
          "name": "InsufficientFunds",
          "type": "_void",
          "description": "The source account does not hold sufficient funds"
        },
        {
          "name": "InvalidSource",
          "type": "_void",
          "description": "The source account was not found or is invalid"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "PaymentRequestErr",
      "name": "PaymentRequestErr",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "ErrEnum { Rejected, InsufficientBalance, Unknown({ reason: str }) }",
      "description": "Error from host_payment_request.",
      "fields": [],
      "variants": [
        {
          "name": "Rejected",
          "type": "_void",
          "description": "User denied the payment request"
        },
        {
          "name": "InsufficientBalance",
          "type": "_void",
          "description": "User's available balance is not sufficient"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "PaymentStatusErr",
      "name": "PaymentStatusErr",
      "category": "Payment",
      "source": "payment.ts",
      "definition": "ErrEnum { PaymentNotFound, Unknown({ reason: str }) }",
      "description": "Error from host_payment_status_subscribe.",
      "fields": [],
      "variants": [
        {
          "name": "PaymentNotFound",
          "type": "_void",
          "description": "Payment ID was not found or does not belong to the current product"
        },
        {
          "name": "Unknown",
          "type": "Struct({ reason: str })",
          "description": "Catch-all"
        }
      ]
    },
    {
      "id": "Entropy",
      "name": "Entropy",
      "category": "Entropy",
      "source": "entropy.ts",
      "definition": "Bytes(32)",
      "description": "32 bytes of deterministic entropy derived from the user's root BIP-39 entropy via a three-layer BLAKE2b-256 keyed hashing scheme.",
      "fields": [],
      "variants": []
    },
    {
      "id": "DeriveEntropyErr",
      "name": "DeriveEntropyErr",
      "category": "Entropy",
      "source": "entropy.ts",
      "definition": "ErrEnum { Unknown }",
      "description": "Error from host_derive_entropy. Under normal operation the function always succeeds; Unknown indicates an unrecoverable internal host error.",
      "fields": [],
      "variants": [
        {
          "name": "Unknown",
          "type": "_void",
          "description": "An unexpected error occurred in the host"
        }
      ]
    },
    {
      "id": "Theme",
      "name": "Theme",
      "category": "Theme",
      "source": "theme.ts",
      "definition": "Status('Light', 'Dark')",
      "description": "Visual theme preference.",
      "fields": [],
      "variants": []
    }
  ],
  "deprecatedAliases": {
    "host_get_non_product_accounts": "host_get_legacy_accounts",
    "host_create_transaction_with_non_product_account": "host_create_transaction_with_legacy_account"
  }
};
export const methods = manifest.methods;
export const groups = manifest.groups;
export const dataTypes = manifest.dataTypes;
export const protocolVersion = manifest.protocol.version;
export default manifest;
