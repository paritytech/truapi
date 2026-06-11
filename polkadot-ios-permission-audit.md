# Polkadot iOS V2 Permission Compliance Audit

Comparison of polkadot-app-ios-v2's TrUAPI handler implementations against the [host permission requirements](docs/host-permission-requirements.md).

## Bugs

| Method | Spec requires | iOS does | Severity |
|--------|--------------|---------|----------|
| `host_get_legacy_accounts` | Return empty array | Returns wallet public keys from `NonProductAccountRegistry` — **leaks identity** | High |

## Correctly implemented

The app is the auth bearer, so authentication is implicit for all methods.

| Method | Permission | Modal |
|--------|:----------:|:-----:|
| `host_account_get` | * | * |
| `host_account_get_alias` | | |
| `host_account_create_proof` | | |
| `host_get_user_id` | * | * |
| `host_sign_payload` | | * |
| `host_sign_raw` | | * |
| `host_create_transaction` | | * |
| `host_push_notification` | * | * |
| `host_push_notification_cancel` | | |
| `host_navigate_to` | * | * |
| `host_device_permission` | * | * |
| `remote_permission` | * | * |
| `remote_statement_store_submit` | * | |
| `remote_preimage_submit` | * | |
| `host_payment_balance_subscribe` | * | * |
| `host_payment_request` | | * |
| `host_payment_top_up` | | * |
| `host_request_resource_allocation` | | * |
| `host_chat_create_room` | | |
| `host_chat_post_message` | | |
| `host_derive_entropy` | | |
| `host_request_login` | | * |
| `host_local_storage_read` | | |
| `host_local_storage_write` | | |
| `host_local_storage_clear` | | |
| `host_theme_subscribe` | | |

## Architecture notes

The permission system uses a two-layer architecture:
- JavaScript layer (`container.js`): Defines method handlers calling native iOS via WebKit bridge
- Native Swift layer: Implements business logic with permission checks
- `ProductPermissionGuard` with `requestPermission()`, `consumePermission()`, `check()` pattern
- Permission states: `notDetermined`, `allowedOnce`, `allowedAlways`, `denied`
- Device capability handler has two-stage check (app-level + OS-level)
- Network access is intercepted at WebView level with domain-based permission checks
- No permission auto-granting
