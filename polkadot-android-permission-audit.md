# Polkadot Android V2 Permission Compliance Audit

Comparison of polkadot-app-android-v2's TrUAPI handler implementations against the [host permission requirements](docs/host-permission-requirements.md).

## Bugs

| Method | Spec requires | Android does | Severity |
|--------|--------------|-------------|----------|
| `host_get_legacy_accounts` | Return empty array | Returns account data — **leaks identity** | High |
| `host_navigate_to` | `DevicePermission::OpenUrl` prompt | **No permission check** — navigates directly | Medium |

## Correctly implemented

The app is the auth bearer, so authentication is implicit for all methods.

| Method | Permission | Modal |
|--------|:----------:|:-----:|
| `host_account_get` | * | * |
| `host_account_get_alias` | * | * |
| `host_get_user_id` | * | * |
| `host_sign_payload` | | * |
| `host_sign_raw` | | * |
| `host_create_transaction` | | * |
| `host_push_notification` | * | * |
| `host_push_notification_cancel` | * | * |
| `host_device_permission` | * | * |
| `remote_permission` | * | * |
| `remote_statement_store_submit` | * | |
| `remote_preimage_submit` | * | |
| `host_payment_balance_subscribe` | * | * |
| `host_payment_request` | | * |
| `host_request_resource_allocation` | | * |
| `host_chat_create_room` | | |
| `host_chat_post_message` | | |
| `host_request_login` | | * |
| `host_local_storage_read` | | |
| `host_local_storage_write` | | |
| `host_local_storage_clear` | | |
| `host_theme_subscribe` | | |

## Architecture notes

The permission system uses a well-structured layered design:
- `ProductPermissionGuard` orchestrates permission decisions
- `ProductPermissionRepository` persists grants and one-time grants
- `ProductPermissionRequester` shows user-facing modals
- `requestPermission()` prompts the user; `consumePermission()` uses a prior one-time grant; `check()` is read-only
- No permission auto-granting was found

## Suggested fixes

### 1. Check OpenUrl device permission in `host_navigate_to` (Medium)

`NavigationHostCalls` navigates without checking `DevicePermission::OpenUrl`. Add a `DeviceCapabilityType.OpenUrl` permission check before proceeding.
