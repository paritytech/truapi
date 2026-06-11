# TrUAPI Permission Compliance — Host Comparison Matrix

Cross-host comparison of permission enforcement against the [spec](docs/host-permission-requirements.md).

## Legend

- OK — Correctly enforces the required permission/prompt
- BUG — Missing a required permission check or prompt
- AUTO — Auto-granted (documented trade-off)
- N/A — Not implemented / returns unsupported error
- — — No permission required by spec

---

## Signing & transaction confirmation

Spec: Auth + user-facing signing modal.

| Method | dotli | Desktop | Android | iOS |
|--------|:-----:|:-------:|:-------:|:---:|
| `host_create_transaction` | OK | OK | OK | OK |
| `host_create_transaction_with_legacy_account` | OK | OK | N/A | N/A |
| `host_sign_payload` | OK | OK | OK | OK |
| `host_sign_payload_with_legacy_account` | OK | OK | N/A | N/A |
| `host_sign_raw` | OK | OK | OK | OK |
| `host_sign_raw_with_legacy_account` | OK | OK | N/A | N/A |

## Identity disclosure

| Method | Spec | dotli | Desktop | Android | iOS |
|--------|------|:-----:|:-------:|:-------:|:---:|
| `host_get_user_id` | Auth + prompt | OK | OK | OK | OK |
| `host_get_legacy_accounts` | Auth, return `[]` | OK | **BUG** | **BUG** | **BUG** |

> Desktop, Android, and iOS return account data instead of an empty array — leaks identity. Only dotli correctly returns `[]`.

## Device permissions

Spec: host prompts once, persists decision.

| Method | Spec | dotli | Desktop | Android | iOS |
|--------|------|:-----:|:-------:|:-------:|:---:|
| `host_navigate_to` | DevicePermission::OpenUrl | **AUTO** | **BUG** | **BUG** | OK |
| `host_push_notification` | DevicePermission::Notifications | OK | OK | OK | OK |
| `host_push_notification_cancel` | DevicePermission::Notifications | OK | OK | OK | OK |
| `host_device_permission` | Explicit prompt | OK | OK | OK | OK |

## Remote permissions

Spec: host prompts once, persists decision.

| Method | Spec | dotli | Desktop | Android | iOS |
|--------|------|:-----:|:-------:|:-------:|:---:|
| `remote_permission` | Explicit prompt | OK | OK | OK | OK |
| `remote_chain_transaction_broadcast` | RemotePermission::ChainSubmit | OK | OK | OK | OK |
| `remote_chain_transaction_stop` | RemotePermission::ChainSubmit | OK | N/A | N/A | N/A |
| `remote_statement_store_submit` | RemotePermission::StatementSubmit | OK | **BUG** | OK | OK |
| `remote_preimage_submit` | RemotePermission::PreimageSubmit | OK | **BUG** | OK | OK |

> dotli: relays `transaction_stop` without checking ChainSubmit permission.
> Desktop: `remote_statement_store_submit` and `remote_preimage_submit` missing permission checks.

## Auth + prompt methods

| Method | Spec | dotli | Desktop | Android | iOS |
|--------|------|:-----:|:-------:|:-------:|:---:|
| `host_payment_balance_subscribe` | Auth + balance disclosure prompt | N/A | N/A | OK | OK |
| `host_payment_request` | Auth + payment confirmation | N/A | N/A | OK | OK |
| `host_payment_top_up` | Auth, source-dependent prompt | N/A | N/A | OK | OK |
| `host_request_resource_allocation` | Auth + per-resource prompt | OK | OK | OK | OK |
| `remote_statement_store_create_proof_authorized` | Auth + pre-allocated allowance | N/A | N/A | OK | OK |

## Auth-only methods (no prompt needed)

| Method | dotli | Desktop | Android | iOS |
|--------|:-----:|:-------:|:-------:|:---:|
| `host_account_get` | OK | OK | OK | OK |
| `host_account_get_alias` | OK | OK | OK | OK |
| `host_account_create_proof` | OK | OK | OK | OK |
| `host_derive_entropy` | OK | OK | OK | OK |
| `host_payment_status_subscribe` | N/A | N/A | OK | OK |

## Chat methods

| Method | Spec | dotli | Desktop | Android | iOS |
|--------|------|:-----:|:-------:|:-------:|:---:|
| `host_chat_create_room` | Auth + prompt | N/A | N/A | OK | OK |
| `host_chat_register_bot` | Auth + prompt | N/A | N/A | N/A | N/A |
| `host_chat_post_message` | Auth | N/A | N/A | OK | OK |
| `host_chat_list_subscribe` | Auth | N/A | N/A | OK | OK |
| `host_chat_action_subscribe` | Auth | N/A | N/A | OK | OK |
| `product_chat_custom_message_render_subscribe` | Auth | N/A | N/A | OK | OK |

## No permission required

| Method | dotli | Desktop | Android | iOS |
|--------|:-----:|:-------:|:-------:|:---:|
| `host_local_storage_read` | OK | OK | OK | OK |
| `host_local_storage_write` | OK | OK | OK | OK |
| `host_local_storage_clear` | OK | OK | OK | OK |
| `host_theme_subscribe` | OK | OK | OK | OK |
| `host_request_login` | OK | OK | OK | OK |
| `host_account_connection_status_subscribe` | OK | OK | OK | OK |
| `host_preimage_lookup_subscribe` | OK | OK | OK | OK |
| `remote_statement_store_subscribe` | OK | OK | OK | OK |
| `remote_preimage_lookup_subscribe` | OK | OK | OK | OK |
| `remote_chain_head_follow_subscribe` | OK | OK | OK | OK |
| `remote_chain_head_header` | OK | OK | OK | OK |
| `remote_chain_head_body` | OK | OK | OK | OK |
| `remote_chain_head_storage` | OK | OK | OK | OK |
| `remote_chain_head_call` | OK | OK | OK | OK |
| `remote_chain_head_unpin` | OK | OK | OK | OK |
| `remote_chain_head_continue` | OK | OK | OK | OK |
| `remote_chain_head_stop_operation` | OK | OK | OK | OK |
| `remote_chain_spec_genesis_hash` | OK | OK | OK | OK |
| `remote_chain_spec_chain_name` | OK | OK | OK | OK |
| `remote_chain_spec_properties` | OK | OK | OK | OK |

---

## Bug summary

| Host | Bugs | Severity |
|------|------|----------|
| **dotli** | `host_navigate_to` auto-granted | 1 Medium |
| **Desktop** | `host_get_legacy_accounts` leaks identity; `remote_preimage_submit` missing PreimageSubmit check; `remote_statement_store_submit` missing StatementSubmit check; `host_navigate_to` missing OpenUrl check | 1 High, 3 Medium |
| **Android** | `host_get_legacy_accounts` leaks identity; `host_navigate_to` missing OpenUrl check | 1 High, 1 Medium |
| **iOS** | `host_get_legacy_accounts` leaks wallet public keys | 1 High |

## Implementation coverage

| Host | Implemented | N/A | Bugs |
|------|:----------:|:---:|:----:|
| **dotli** | 28 | 14 | 1 |
| **Desktop** | 28 | 14 | 4 |
| **Android** | 36 | 6 | 2 |
| **iOS** | 36 | 6 | 1 |
