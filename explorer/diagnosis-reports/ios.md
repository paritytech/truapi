## Truapi iOS Diagnosis

| Method                                         | Status | Details                                                                          |
| ---------------------------------------------- | ------ | -------------------------------------------------------------------------------- |
| Account/connection_status_subscribe            | ✅     |                                                                                  |
| Account/get_account                            | ✅     |                                                                                  |
| Account/get_account_alias                      | ✅     |                                                                                  |
| Account/create_account_proof                   | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                   |
| Account/get_legacy_accounts                    | ✅     |                                                                                  |
| Account/get_user_id                            | ✅     |                                                                                  |
| Account/request_login                          | ✅     |                                                                                  |
| Chain/follow_head_subscribe                    | ✅     |                                                                                  |
| Chain/get_head_header                          | ✅     |                                                                                  |
| Chain/get_head_body                            | ✅     |                                                                                  |
| Chain/get_head_storage                         | ✅     |                                                                                  |
| Chain/call_head                                | ✅     |                                                                                  |
| Chain/unpin_head                               | ✅     |                                                                                  |
| Chain/continue_head                            | ✅     |                                                                                  |
| Chain/stop_head_operation                      | ✅     |                                                                                  |
| Chain/get_spec_genesis_hash                    | ✅     |                                                                                  |
| Chain/get_spec_chain_name                      | ✅     |                                                                                  |
| Chain/get_spec_properties                      | ✅     |                                                                                  |
| Chain/broadcast_transaction                    | ✅     |                                                                                  |
| Chain/stop_transaction                         | ✅     |                                                                                  |
| Chat/create_room                               | ❌     | timed out after 10s                                                              |
| Chat/register_bot                              | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                   |
| Chat/list_subscribe                            | ❌     | subscription delivered no events in 6s                                           |
| Chat/post_message                              | ❌     | { "tag": "Unknown", "value": { "reason": "Error: Messages are not supported" } } |
| Chat/action_subscribe                          | ❌     | subscription delivered no events in 6s                                           |
| Chat/custom_message_render_subscribe           | ❌     | subscription delivered no events in 6s                                           |
| Entropy/derive                                 | ✅     |                                                                                  |
| Local Storage/read                             | ✅     |                                                                                  |
| Local Storage/write                            | ✅     |                                                                                  |
| Local Storage/clear                            | ✅     |                                                                                  |
| Notifications/send_push_notification           | ✅     |                                                                                  |
| Notifications/cancel_push_notification         | ✅     |                                                                                  |
| Payment/balance_subscribe                      | ✅     |                                                                                  |
| Payment/top_up                                 | ✅     |                                                                                  |
| Payment/request                                | ✅     |                                                                                  |
| Payment/status_subscribe                       | ✅     |                                                                                  |
| Permissions/request_device_permission          | ✅     |                                                                                  |
| Permissions/request_remote_permission          | ✅     |                                                                                  |
| Preimage/lookup_subscribe                      | ✅     |                                                                                  |
| Preimage/submit                                | ✅     |                                                                                  |
| Resource Allocation/request                    | ✅     |                                                                                  |
| Signing/create_transaction                     | ✅     |                                                                                  |
| Signing/create_transaction_with_legacy_account | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                   |
| Signing/sign_raw_with_legacy_account           | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                   |
| Signing/sign_payload_with_legacy_account       | ❌     | { "tag": "Unknown", "value": { "reason": "Not implemented" } }                   |
| Signing/sign_raw                               | ✅     |                                                                                  |
| Signing/sign_payload                           | ✅     |                                                                                  |
| Statement Store/subscribe                      | ❌     | createProof failed: { "tag": "UnableToSign" }                                    |
| Statement Store/create_proof                   | ❌     | { "tag": "UnableToSign" }                                                        |
| Statement Store/submit                         | ❌     | createProof failed: { "tag": "UnableToSign" }                                    |
| Statement Store/create_proof_authorized        | ✅     |                                                                                  |
| System/handshake                               | ✅     |                                                                                  |
| System/feature_supported                       | ✅     |                                                                                  |
| System/navigate_to                             | ✅     |                                                                                  |
| Theme/subscribe                                | ✅     |                                                                                  |
