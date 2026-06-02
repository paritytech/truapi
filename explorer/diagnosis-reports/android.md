## Truapi Android Diagnosis

_Generated: 2026-06-02T13:24:09.016Z_

| Method                                           | Status | Details                                                                                                         |
| ------------------------------------------------ | ------ | --------------------------------------------------------------------------------------------------------------- |
| `Account/connection_status_subscribe`            | ✅     |                                                                                                                 |
| `Account/get_account`                            | ✅     |                                                                                                                 |
| `Account/get_account_alias`                      | ✅     |                                                                                                                 |
| `Account/create_account_proof`                   | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                                  |
| `Account/get_legacy_accounts`                    | ✅     |                                                                                                                 |
| `Account/get_user_id`                            | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                                  |
| `Account/request_login`                          | ✅     |                                                                                                                 |
| `Chain/follow_head_subscribe`                    | ✅     |                                                                                                                 |
| `Chain/get_head_header`                          | ✅     |                                                                                                                 |
| `Chain/get_head_body`                            | ✅     |                                                                                                                 |
| `Chain/get_head_storage`                         | ✅     |                                                                                                                 |
| `Chain/call_head`                                | ✅     |                                                                                                                 |
| `Chain/unpin_head`                               | ✅     |                                                                                                                 |
| `Chain/continue_head`                            | ✅     |                                                                                                                 |
| `Chain/stop_head_operation`                      | ✅     |                                                                                                                 |
| `Chain/get_spec_genesis_hash`                    | ✅     |                                                                                                                 |
| `Chain/get_spec_chain_name`                      | ✅     |                                                                                                                 |
| `Chain/get_spec_properties`                      | ✅     |                                                                                                                 |
| `Chain/broadcast_transaction`                    | ✅     |                                                                                                                 |
| `Chain/stop_transaction`                         | ✅     |                                                                                                                 |
| `Chat/create_room`                               | ❌     | timed out after 10s                                                                                             |
| `Chat/register_bot`                              | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                                  |
| `Chat/list_subscribe`                            | ❌     | subscription delivered no events in 6s                                                                          |
| `Chat/post_message`                              | ❌     | { "tag": "Unknown", "value": { "reason": "Error: Unknown method: chatSendTextMessage" } }                       |
| `Chat/action_subscribe`                          | ❌     | subscription delivered no events in 6s                                                                          |
| `Chat/custom_message_render_subscribe`           | ❌     | subscription delivered no events in 6s                                                                          |
| `Coin Payment/create_purse`                      | ⏭     |                                                                                                                 |
| `Coin Payment/query_purse`                       | ⏭     |                                                                                                                 |
| `Coin Payment/rebalance_purse`                   | ⏭     |                                                                                                                 |
| `Coin Payment/delete_purse`                      | ⏭     |                                                                                                                 |
| `Coin Payment/create_receivable`                 | ⏭     |                                                                                                                 |
| `Coin Payment/create_cheque`                     | ⏭     |                                                                                                                 |
| `Coin Payment/deposit`                           | ⏭     |                                                                                                                 |
| `Coin Payment/refund`                            | ⏭     |                                                                                                                 |
| `Coin Payment/listen_for_payment`                | ⏭     |                                                                                                                 |
| `Entropy/derive`                                 | ✅     |                                                                                                                 |
| `Local Storage/read`                             | ✅     |                                                                                                                 |
| `Local Storage/write`                            | ✅     |                                                                                                                 |
| `Local Storage/clear`                            | ✅     |                                                                                                                 |
| `Notifications/send_push_notification`           | ❌     | timed out after 10s                                                                                             |
| `Notifications/cancel_push_notification`         | ✅     |                                                                                                                 |
| `Payment/balance_subscribe`                      | ✅     |                                                                                                                 |
| `Payment/top_up`                                 | ✅     |                                                                                                                 |
| `Payment/request`                                | ❌     | request failed: { "tag": "InsufficientBalance" }                                                                |
| `Payment/status_subscribe`                       | ❌     | request failed: { "tag": "InsufficientBalance" }                                                                |
| `Permissions/request_device_permission`          | ✅     |                                                                                                                 |
| `Permissions/request_remote_permission`          | ✅     |                                                                                                                 |
| `Preimage/lookup_subscribe`                      | ✅     |                                                                                                                 |
| `Preimage/submit`                                | ❌     | timed out after 10s                                                                                             |
| `Resource Allocation/request`                    | ✅     |                                                                                                                 |
| `Signing/create_transaction`                     | ❌     | { "tag": "Unknown", "value": { "reason": "Error: Internal error: User rejected" } }                             |
| `Signing/create_transaction_with_legacy_account` | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                                  |
| `Signing/sign_raw_with_legacy_account`           | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                                  |
| `Signing/sign_payload_with_legacy_account`       | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                                                  |
| `Signing/sign_raw`                               | ✅     |                                                                                                                 |
| `Signing/sign_payload`                           | ✅     |                                                                                                                 |
| `Statement Store/subscribe`                      | ❌     | subscription delivered no events in 6s                                                                          |
| `Statement Store/create_proof`                   | ✅     |                                                                                                                 |
| `Statement Store/submit`                         | ❌     | submit failed: { "reason": "Error: Internal error: Fatal statement store submission error: Invalid(BadProof)" } |
| `Statement Store/create_proof_authorized`        | ✅     |                                                                                                                 |
| `System/handshake`                               | ✅     |                                                                                                                 |
| `System/feature_supported`                       | ✅     |                                                                                                                 |
| `System/navigate_to`                             | ✅     |                                                                                                                 |
| `Theme/subscribe`                                | ✅     |                                                                                                                 |
