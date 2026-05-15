export type TrUApiMethodKind = "request" | "subscription" | "reverse-subscription";
export type TrUApiMethodName = "host_feature_supported" | "host_navigate_to" | "host_push_notification" | "host_device_permission" | "remote_permission" | "host_local_storage_read" | "host_local_storage_write" | "host_local_storage_clear" | "host_account_get" | "host_account_get_alias" | "host_account_create_proof" | "host_get_legacy_accounts" | "host_account_connection_status_subscribe" | "host_get_user_id" | "host_request_login" | "host_sign_payload" | "host_sign_raw" | "host_sign_raw_with_legacy_account" | "host_sign_payload_with_legacy_account" | "host_create_transaction" | "host_create_transaction_with_legacy_account" | "host_chat_create_room" | "host_chat_create_simple_group" | "host_chat_register_bot" | "host_chat_post_message" | "host_chat_list_subscribe" | "host_chat_action_subscribe" | "product_chat_custom_message_render_subscribe" | "remote_statement_store_subscribe" | "remote_statement_store_create_proof" | "remote_statement_store_submit" | "remote_preimage_lookup_subscribe" | "remote_chain_head_follow" | "remote_chain_head_header" | "remote_chain_head_body" | "remote_chain_head_storage" | "remote_chain_head_call" | "remote_chain_head_unpin" | "remote_chain_head_continue" | "remote_chain_head_stop_operation" | "remote_chain_spec_genesis_hash" | "remote_chain_spec_chain_name" | "remote_chain_spec_properties" | "remote_chain_transaction_broadcast" | "remote_chain_transaction_stop" | "host_payment_balance_subscribe" | "host_payment_top_up" | "host_payment_request" | "host_payment_status_subscribe" | "host_derive_entropy" | "host_theme_subscribe";
export type TrUApiDataTypeName = "str" | "bool" | "u8" | "u32" | "u64" | "u128" | "compact" | "Hex" | "Bytes" | "BytesN" | "_void" | "Option" | "Nullable" | "Vector" | "Tuple" | "Struct" | "Enum" | "Status" | "Result" | "ErrEnum" | "GenesisHash" | "GenericErr" | "GenericError" | "AccountId" | "PublicKey" | "DotNsIdentifier" | "DerivationIndex" | "ProductAccountId" | "ProductAccount" | "LegacyAccount" | "ContextualAlias" | "RingLocationHint" | "RingLocation" | "RingVrfProof" | "AccountConnectionStatus" | "LoginResult" | "LoginError" | "UserIdentity" | "UserIdentityErr" | "RequestCredentialsErr" | "CreateProofErr" | "SigningPayload" | "RawPayload" | "SigningRawPayload" | "SigningPayloadPayload" | "SigningRawPayloadWithoutAccount" | "SigningPayloadWithoutAccount" | "SigningResult" | "SigningErr" | "TxPayloadExtensionV1" | "TxPayloadContextV1" | "TxPayloadV1" | "VersionedTxPayload" | "CreateTransactionErr" | "StorageKey" | "StorageValue" | "StorageErr" | "NavigateToErr" | "PushNotification" | "DevicePermission" | "RemotePermission" | "Feature" | "ChatRoomRequest" | "ChatRoomRegistrationStatus" | "ChatRoomRegistrationResult" | "SimpleGroupChatRequest" | "SimpleGroupChatResult" | "ChatBotRequest" | "ChatBotRegistrationStatus" | "ChatBotRegistrationResult" | "ChatRoomParticipation" | "ChatRoom" | "ChatAction" | "ChatActionLayout" | "ChatActions" | "ChatMedia" | "ChatRichText" | "ChatFile" | "ChatReaction" | "ChatCustomMessage" | "ChatMessageContent" | "ChatPostMessageResult" | "ActionTrigger" | "ChatCommand" | "ChatActionPayload" | "ReceivedChatAction" | "ChatRoomRegistrationErr" | "ChatBotRegistrationErr" | "ChatMessagePostingErr" | "Size" | "Dimensions" | "TypographyStyle" | "ButtonVariant" | "ColorToken" | "ContentAlignment" | "HorizontalAlignment" | "VerticalAlignment" | "Arrangement" | "Shape" | "BorderStyle" | "Modifier" | "CustomRendererNode" | "BlockHash" | "OperationId" | "RuntimeApi" | "RuntimeSpec" | "RuntimeType" | "StorageQueryType" | "StorageQueryItem" | "StorageResultItem" | "OperationStartedResult" | "ChainHeadEvent" | "Topic" | "Channel" | "DecryptionKey" | "TopicFilter" | "StatementProof" | "Statement" | "SignedStatement" | "SignedStatementsPage" | "StatementProofErr" | "PreimageKey" | "PreimageValue" | "Balance" | "PaymentId" | "Ed25519PrivateKey" | "PaymentBalance" | "PaymentTopUpSource" | "PaymentReceipt" | "PaymentStatus" | "PaymentBalanceErr" | "PaymentTopUpErr" | "PaymentRequestErr" | "PaymentStatusErr" | "Entropy" | "DeriveEntropyErr" | "Theme";

export interface TrUApiMethodArtifact {
  readonly name: TrUApiMethodName;
  readonly tag: number;
  readonly kind: TrUApiMethodKind;
  readonly group: string;
  readonly request: string;
  readonly response: string;
  readonly errorType: string | null;
}

export interface TrUApiGroupArtifact {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly methods: readonly TrUApiMethodName[];
}

export interface TrUApiDataTypeArtifact {
  readonly id: TrUApiDataTypeName;
  readonly name: string;
  readonly category: string;
  readonly source: string | null;
  readonly definition: string;
  readonly description: string;
  readonly fields: readonly { readonly name: string; readonly type: string; readonly description: string }[];
  readonly variants: readonly { readonly name: string; readonly type: string; readonly description: string }[];
}

export interface TrUApiManifest {
  readonly schemaVersion: 1;
  readonly protocol: {
    readonly name: "TrUAPI";
    readonly version: "0.2";
    readonly source: { readonly repo: string; readonly path: string; readonly revision: string };
    readonly transport: "message-port";
    readonly wireFormat: "scale-host-api";
  };
  readonly methods: readonly TrUApiMethodArtifact[];
  readonly groups: readonly TrUApiGroupArtifact[];
  readonly dataTypes: readonly TrUApiDataTypeArtifact[];
  readonly deprecatedAliases: Readonly<Record<string, TrUApiMethodName>>;
}

export declare const manifest: TrUApiManifest;
export declare const methods: readonly TrUApiMethodArtifact[];
export declare const groups: readonly TrUApiGroupArtifact[];
export declare const dataTypes: readonly TrUApiDataTypeArtifact[];
export declare const protocolVersion: "0.2";
export default manifest;
