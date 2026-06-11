# Polkadot Desktop Permission Compliance Audit

Comparison of polkadot-desktop's TrUAPI handler implementations against the [host permission requirements](docs/host-permission-requirements.md).

## Bugs

| Method | Spec requires | Polkadot Desktop does | Severity |
|--------|--------------|----------------------|----------|
| `host_get_legacy_accounts` | Return empty array | Returns account data — **leaks identity** | High |
| `host_navigate_to` | `DevicePermission::OpenUrl` prompt | **No permission check** — opens URL directly via shell | Medium |
| `remote_statement_store_submit` | `RemotePermission::StatementSubmit` check | **No permission check** in handler | Medium |
| `remote_preimage_submit` | `RemotePermission::PreimageSubmit` check | Shows modal but **no PreimageSubmit permission check** | Medium |

## Correctly implemented

| Method | Auth | Permission | Modal |
|--------|:----:|:----------:|:-----:|
| `host_account_get` | * | | |
| `host_account_get_alias` | * | * | * |
| `host_account_create_proof` | * | | |
| `host_get_user_id` | * | * | |
| `host_create_transaction` | * | * | * |
| `host_create_transaction_with_legacy_account` | * | * | * |
| `host_sign_payload` | * | * | * |
| `host_sign_payload_with_legacy_account` | * | * | * |
| `host_sign_raw` | * | * | * |
| `host_sign_raw_with_legacy_account` | * | * | * |
| `host_derive_entropy` | * | | |
| `host_request_resource_allocation` | * | | * |
| `host_device_permission` | | * | * |
| `remote_permission` | | * | * |
| `host_request_login` | | | * |
| `host_local_storage_read` | | | |
| `host_local_storage_write` | | | |
| `host_local_storage_clear` | | | |
| `host_push_notification` | | * | |
| `host_push_notification_cancel` | | * | |
| `host_theme_subscribe` | | | |
| `host_preimage_lookup_subscribe` | | | |

## Suggested fixes

### 1. Check OpenUrl device permission in `host_navigate_to` (Medium)

The handler opens URLs via the shell without checking `DevicePermission::OpenUrl`. Add a permission check before opening the URL.

### 2. Check StatementSubmit remote permission in `remote_statement_store_submit` (Medium)

The handler submits statements without verifying `RemotePermission::StatementSubmit`. Add the permission check before proceeding.

### 3. Check PreimageSubmit remote permission in `remote_preimage_submit` (Medium)

The handler shows a user modal but does not check `RemotePermission::PreimageSubmit` before proceeding. Add the permission check.
