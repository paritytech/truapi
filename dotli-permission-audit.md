# Dotli Permission Compliance Audit

Comparison of dotli's TrUAPI handler implementations against the [host permission requirements](docs/host-permission-requirements.md).

## Bugs

| Method | Spec requires | Dotli does | Severity |
|--------|--------------|------------|----------|
| `host_navigate_to` | `DevicePermission::OpenUrl` prompt | Auto-granted, no prompt | Medium |

## Auto-granted permissions (documented trade-offs)

These are intentional deviations from the spec, documented in `packages/ui/src/permissions.ts`:

| Permission | Spec requires | Dotli behavior | Reason |
|------------|--------------|----------------|--------|
| `RemotePermission::Remote` | User prompt | Auto-granted | Browser cannot intercept fetch/XHR from inside an iframe |
| `RemotePermission::WebRtc` | User prompt | Auto-granted | Already gated by iframe `allow` attribute |
| `DevicePermission::OpenUrl` | User prompt | Auto-granted | Cross-origin navigation cannot be blocked by the host |

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
| `host_sign_raw` | * | | * |
| `host_sign_raw_with_legacy_account` | * | | * |
| `host_derive_entropy` | * | | |
| `host_request_resource_allocation` | * | | * |
| `host_device_permission` | | * | * |
| `remote_permission` | | * | * |
| `host_request_login` | | | * |
| `host_local_storage_read` | | | |
| `host_local_storage_write` | | | |
| `host_local_storage_clear` | | | |
| `host_theme_subscribe` | | | |
| `host_preimage_lookup_subscribe` | | | |
| `host_push_notification` | | * | |
| `host_push_notification_cancel` | | * | |
| `host_preimage_submit` | | | * |

## Suggested fixes

### 1. `host_navigate_to` auto-granted (Medium)

Auto-granted due to browser limitations (documented trade-off).
