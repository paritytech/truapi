## Truapi Web Diagnosis

| Method                                           | Status | Details                                                                                           |
| ------------------------------------------------ | ------ | ------------------------------------------------------------------------------------------------- |
| `Account/connection_status_subscribe`            | ✅     |                                                                                                   |
| `Account/get_account`                            | ✅     |                                                                                                   |
| `Account/get_account_alias`                      | ✅     |                                                                                                   |
| `Account/create_account_proof`                   | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                    |
| `Account/get_legacy_accounts`                    | ✅     |                                                                                                   |
| `Account/get_user_id`                            | ✅     |                                                                                                   |
| `Account/request_login`                          | ✅     |                                                                                                   |
| `Chain/follow_head_subscribe`                    | ✅     |                                                                                                   |
| `Chain/get_head_header`                          | ✅     |                                                                                                   |
| `Chain/get_head_body`                            | ✅     |                                                                                                   |
| `Chain/get_head_storage`                         | ✅     |                                                                                                   |
| `Chain/call_head`                                | ✅     |                                                                                                   |
| `Chain/unpin_head`                               | ✅     |                                                                                                   |
| `Chain/continue_head`                            | ✅     |                                                                                                   |
| `Chain/stop_head_operation`                      | ✅     |                                                                                                   |
| `Chain/get_spec_genesis_hash`                    | ✅     |                                                                                                   |
| `Chain/get_spec_chain_name`                      | ✅     |                                                                                                   |
| `Chain/get_spec_properties`                      | ✅     |                                                                                                   |
| `Chain/broadcast_transaction`                    | ✅     |                                                                                                   |
| `Chain/stop_transaction`                         | ✅     |                                                                                                   |
| `Chat/create_room`                               | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                    |
| `Chat/register_bot`                              | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                    |
| `Chat/list_subscribe`                            | ✅     |                                                                                                   |
| `Chat/post_message`                              | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                    |
| `Chat/action_subscribe`                          | ✅     |                                                                                                   |
| `Chat/custom_message_render_subscribe`           | ❌     | subscription delivered no events in 6s                                                            |
| `Entropy/derive`                                 | ✅     |                                                                                                   |
| `Local Storage/read`                             | ✅     |                                                                                                   |
| `Local Storage/write`                            | ✅     |                                                                                                   |
| `Local Storage/clear`                            | ✅     |                                                                                                   |
| `Notifications/send_push_notification`           | ✅     |                                                                                                   |
| `Notifications/cancel_push_notification`         | ✅     |                                                                                                   |
| `Payment/balance_subscribe`                      | ❌     | { "name": "SubscriptionError", "reason": { "tag": "PermissionDenied" } }                          |
| `Payment/top_up`                                 | ❌     | { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } }               |
| `Payment/request`                                | ❌     | topUp failed: { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } } |
| `Payment/status_subscribe`                       | ❌     | topUp failed: { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } } |
| `Permissions/request_device_permission`          | ✅     |                                                                                                   |
| `Permissions/request_remote_permission`          | ✅     |                                                                                                   |
| `Preimage/lookup_subscribe`                      | ✅     |                                                                                                   |
| `Preimage/submit`                                | ✅     |                                                                                                   |
| `Resource Allocation/request`                    | ✅     |                                                                                                   |
| `Signing/create_transaction`                     | ✅     |                                                                                                   |
| `Signing/create_transaction_with_legacy_account` | ❌     | { "tag": "Unknown", "value": { "reason": "Account can't be derived from product account id" } }   |
| `Signing/sign_raw_with_legacy_account`           | ❌     | { "tag": "Unknown", "value": { "reason": "Account can't be derived from product account id" } }   |
| `Signing/sign_payload_with_legacy_account`       | ❌     | { "tag": "Unknown", "value": { "reason": "Account can't be derived from product account id" } }   |
| `Signing/sign_raw`                               | ✅     |                                                                                                   |
| `Signing/sign_payload`                           | ✅     |                                                                                                   |
| `Statement Store/subscribe`                      | ✅     |                                                                                                   |
| `Statement Store/create_proof`                   | ✅     |                                                                                                   |
| `Statement Store/submit`                         | ❌     | submit failed: { "reason": "Submit failed, statement already expired" }                           |
| `Statement Store/create_proof_authorized`        | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                    |
| `System/handshake`                               | ✅     |                                                                                                   |
| `System/feature_supported`                       | ✅     |                                                                                                   |
| `System/navigate_to`                             | ✅     |                                                                                                   |
| `Theme/subscribe`                                | ✅     |                                                                                                   |
