# TrUAPI Host Permission Requirements

Every TrUAPI method falls into one of four permission tiers. Hosts **must** enforce the requirements listed below before executing each call.

## Legend

| Column | Meaning |
|--------|---------|
| **Auth** | User must be logged in (`NotConnected` error if not) |
| **Prompt** | Host must show a user-facing confirmation UI before proceeding |
| **Permission type** | Which permission system governs the prompt |

---

## 1. No permission required

These methods work without login and without any user prompt.

| Method | Notes |
|--------|-------|
| `host_handshake` | Protocol negotiation |
| `host_feature_supported` | Capability query |
| `host_local_storage_read` | Product-scoped storage |
| `host_local_storage_write` | Product-scoped storage |
| `host_local_storage_clear` | Product-scoped storage |
| `host_theme_subscribe` | UI theming |
| `host_account_connection_status_subscribe` | Read-only status |
| `host_request_login` | Presents login UI (user controls outcome) |
| `remote_chain_head_follow_subscribe` | Read-only chain data |
| `remote_chain_head_header` | Read-only chain data |
| `remote_chain_head_body` | Read-only chain data |
| `remote_chain_head_storage` | Read-only chain data |
| `remote_chain_head_call` | Read-only chain data |
| `remote_chain_head_unpin` | Read-only chain data |
| `remote_chain_head_continue` | Read-only chain data |
| `remote_chain_head_stop_operation` | Read-only chain data |
| `remote_chain_spec_genesis_hash` | Read-only chain data |
| `remote_chain_spec_chain_name` | Read-only chain data |
| `remote_chain_spec_properties` | Read-only chain data |
| `remote_statement_store_subscribe` | Read-only statement data |
| `remote_preimage_lookup_subscribe` | Read-only preimage data |

---

## 2. Authentication required (no additional prompt)

User must be logged in. The host does **not** show a separate permission prompt — access is granted to any authenticated product.

| Method | Error on no auth |
|--------|------------------|
| `host_account_get` | `NotConnected` / `Rejected` |
| `host_account_get_alias` | `NotConnected` / `Rejected` |
| `host_account_create_proof` | `NotConnected` / `Rejected` |
| `host_derive_entropy` | `Unknown` |
| `host_get_legacy_accounts` | `Rejected` |
| `host_chat_list_subscribe` | Requires active session |
| `host_chat_action_subscribe` | Requires active session |
| `product_chat_custom_message_render_subscribe` | Requires active session |
| `host_payment_status_subscribe` | `PaymentNotFound` |

---

## 3. Authentication + user confirmation prompt

These methods require login **and** an explicit user-facing prompt before proceeding. The host must present a confirmation UI and return `Rejected` / `PermissionDenied` / `Denied` if the user declines.

### 3a. Signing & transaction confirmation

The host shows the user what is being signed or submitted, and the user approves or rejects.

| Method | Prompt trigger | Error on denial |
|--------|---------------|-----------------|
| `host_create_transaction` | Always — user reviews transaction details | `Rejected` / `PermissionDenied` |
| `host_create_transaction_with_legacy_account` | Always — user reviews transaction details | `Rejected` / `PermissionDenied` |
| `host_sign_raw` | Always — user reviews payload | `Rejected` / `PermissionDenied` |
| `host_sign_payload` | Always — user reviews extrinsic payload | `Rejected` / `PermissionDenied` |
| `host_sign_raw_with_legacy_account` | Always — user reviews payload | `Rejected` / `PermissionDenied` |
| `host_sign_payload_with_legacy_account` | Always — user reviews extrinsic payload | `Rejected` / `PermissionDenied` |

### 3b. Identity disclosure

| Method | Prompt trigger | Error on denial |
|--------|---------------|-----------------|
| `host_get_user_id` | Always — user approves revealing their primary DotNS name to the product | `PermissionDenied` |

### 3c. Payment confirmation

| Method | Prompt trigger | Error on denial |
|--------|---------------|-----------------|
| `host_payment_balance_subscribe` | First call — user approves balance disclosure | `PermissionDenied` |
| `host_payment_request` | Always — user approves spend | `Rejected` |
| `host_payment_top_up` | Depends on source — host may prompt for `ProductAccount` source | `InsufficientFunds` / `InvalidSource` |

### 3d. Chat room & bot registration

| Method | Prompt trigger | Error on denial |
|--------|---------------|-----------------|
| `host_chat_create_room` | Host may prompt on first room creation | `PermissionDenied` |
| `host_chat_register_bot` | Host may prompt on first bot registration | `PermissionDenied` |
| `host_chat_post_message` | No prompt (already authorized by room creation) | `MessageTooLarge` |

### 3e. Statement store proof creation

| Method | Prompt trigger | Error on denial |
|--------|---------------|-----------------|
| ~~`remote_statement_store_create_proof`~~ | **Deprecated** — use `create_proof_authorized` instead | — |
| `remote_statement_store_create_proof_authorized` | No per-call prompt — uses pre-allocated `AutoSigning` allowance from `host_request_resource_allocation` | `UnableToSign` |

---

## 4. Device & remote permissions (RFC 0002)

These permissions use the RFC 0002 permission model: the host prompts once, persists the user's decision indefinitely, and does not re-prompt on subsequent requests.

### 4a. Explicit permission requests

Products may pre-request permissions; the host shows a one-time prompt.

| Method | Permission |
|--------|------------|
| `host_device_permission` | Requests one `DevicePermission` variant |
| `remote_permission` | Requests one `RemotePermission` variant |

**`DevicePermission` variants:** `Notifications`, `Camera`, `Microphone`, `Bluetooth`, `NFC`, `Location`, `Clipboard`, `OpenUrl`, `Biometrics`

**`RemotePermission` variants:** `Remote { domains }`, `WebRtc`, `ChainSubmit`, `PreimageSubmit`, `StatementSubmit`

### 4b. Implicit permission triggers

These business methods **automatically trigger** a remote permission prompt if the corresponding permission has not been granted yet. The host should prompt for the permission before executing the call.

| Method | Implicitly requires | Error on denial |
|--------|-------------------|-----------------|
| `host_navigate_to` | `DevicePermission::OpenUrl` | `PermissionDenied` |
| `host_push_notification` | `DevicePermission::Notifications` | `Unknown` |
| `host_push_notification_cancel` | `DevicePermission::Notifications` (same grant) | `Unknown` |
| `remote_chain_transaction_broadcast` | `RemotePermission::ChainSubmit` | `GenericError` |
| `remote_chain_transaction_stop` | `RemotePermission::ChainSubmit` (same grant) | `GenericError` |
| `remote_preimage_submit` | `RemotePermission::PreimageSubmit` | `GenericError` |
| `remote_statement_store_submit` | `RemotePermission::StatementSubmit` | `GenericError` |

### 4c. Resource allocation

Pre-allocates capabilities that relax per-call prompts for subsequent operations.

| Method | Notes |
|--------|-------|
| `host_request_resource_allocation` | User approves each `AllocatableResource`. Grants like `AutoSigning` enable `create_proof_authorized` without per-call prompts. Per-resource outcome: `Allocated` / `Rejected` / `NotAvailable`. |

**`AllocatableResource` variants:** `StatementStoreAllowance`, `BulletinAllowance`, `SmartContractAllowance`, `AutoSigning`

---

## Quick reference matrix

| Method | Auth | Prompt | Permission type |
|--------|:----:|:------:|----------------|
| `host_handshake` | | | — |
| `host_feature_supported` | | | — |
| `host_push_notification` | | | DevicePermission::Notifications |
| `host_push_notification_cancel` | | | DevicePermission::Notifications |
| `host_navigate_to` | | | DevicePermission::OpenUrl |
| `host_device_permission` | | | Explicit prompt |
| `remote_permission` | | | Explicit prompt |
| `host_local_storage_read` | | | — |
| `host_local_storage_write` | | | — |
| `host_local_storage_clear` | | | — |
| `host_account_connection_status_subscribe` | | | — |
| `host_account_get` | * | | — |
| `host_account_get_alias` | * | | — |
| `host_account_create_proof` | * | | — |
| `host_get_legacy_accounts` | * | | — |
| `host_create_transaction` | * | * | Signing confirmation |
| `host_create_transaction_with_legacy_account` | * | * | Signing confirmation |
| `host_sign_raw_with_legacy_account` | * | * | Signing confirmation |
| `host_sign_payload_with_legacy_account` | * | * | Signing confirmation |
| `host_chat_create_room` | * | * | Chat registration |
| `host_chat_register_bot` | * | * | Chat registration |
| `host_chat_list_subscribe` | * | | — |
| `host_chat_post_message` | * | | — |
| `host_chat_action_subscribe` | * | | — |
| `product_chat_custom_message_render_subscribe` | * | | — |
| `remote_statement_store_subscribe` | | | — |
| ~~`remote_statement_store_create_proof`~~ | * | * | **Deprecated** |
| `remote_statement_store_create_proof_authorized` | * | | Pre-allocated allowance |
| `remote_statement_store_submit` | | | RemotePermission::StatementSubmit |
| `remote_preimage_lookup_subscribe` | | | — |
| `remote_preimage_submit` | | | RemotePermission::PreimageSubmit |
| `remote_chain_head_follow_subscribe` | | | — |
| `remote_chain_head_header` | | | — |
| `remote_chain_head_body` | | | — |
| `remote_chain_head_storage` | | | — |
| `remote_chain_head_call` | | | — |
| `remote_chain_head_unpin` | | | — |
| `remote_chain_head_continue` | | | — |
| `remote_chain_head_stop_operation` | | | — |
| `remote_chain_spec_genesis_hash` | | | — |
| `remote_chain_spec_chain_name` | | | — |
| `remote_chain_spec_properties` | | | — |
| `remote_chain_transaction_broadcast` | | | RemotePermission::ChainSubmit |
| `remote_chain_transaction_stop` | | | RemotePermission::ChainSubmit |
| `host_theme_subscribe` | | | — |
| `host_derive_entropy` | * | | — |
| `host_get_user_id` | * | * | Identity disclosure |
| `host_request_login` | | | — (presents login UI) |
| `host_sign_raw` | * | * | Signing confirmation |
| `host_sign_payload` | * | * | Signing confirmation |
| `host_payment_balance_subscribe` | * | * | Balance disclosure |
| `host_payment_top_up` | * | | Source-dependent |
| `host_payment_request` | * | * | Payment confirmation |
| `host_payment_status_subscribe` | * | | — |
| `host_request_resource_allocation` | * | * | Per-resource prompt |
