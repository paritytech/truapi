## Truapi CLI Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Account/connection_status_subscribe` | ✅ |  |
| `Account/get_account` | ✅ |  |
| `Account/get_account_alias` | ✅ |  |
| `Account/create_account_proof` | ✅ |  |
| `Account/get_legacy_accounts` | ✅ |  |
| `Account/get_user_id` | ✅ |  |
| `Account/request_login` | ✅ |  |
| `Chain/follow_head_subscribe` | ✅ |  |
| `Chain/get_head_header` | ✅ |  |
| `Chain/get_head_body` | ✅ |  |
| `Chain/get_head_storage` | ✅ |  |
| `Chain/call_head` | ✅ |  |
| `Chain/unpin_head` | ✅ |  |
| `Chain/continue_head` | ✅ |  |
| `Chain/stop_head_operation` | ✅ |  |
| `Chain/get_spec_genesis_hash` | ✅ |  |
| `Chain/get_spec_chain_name` | ✅ |  |
| `Chain/get_spec_properties` | ✅ |  |
| `Chain/broadcast_transaction` | ✅ |  |
| `Chain/stop_transaction` | ✅ |  |
| `Chat/create_room` | ❌ | createRoom failed: { "error": { "tag": "HostFailure", "value": { "reason": "unavailable" } } } |
| `Chat/register_bot` | ❌ | registerBot failed: { "error": { "tag": "HostFailure", "value": { "reason": "unavailable" } } } |
| `Chat/list_subscribe` | ❌ | no elements in sequence |
| `Chat/post_message` | ❌ | postMessage failed: { "error": { "tag": "HostFailure", "value": { "reason": "unavailable" } } } |
| `Chat/action_subscribe` | ❌ | no elements in sequence |
| `Chat/custom_message_render_subscribe` | ❌ | no elements in sequence |
| `Coin Payment/create_purse` | ❌ | createPurse failed: { "error": { "tag": "HostFailure", "value": { "reason": "unavailable" } } } |
| `Coin Payment/query_purse` | ❌ | queryPurse failed: { "error": { "tag": "HostFailure", "value": { "reason": "unavailable" } } } |
| `Coin Payment/rebalance_purse` | ❌ | Subscription interrupted |
| `Coin Payment/delete_purse` | ❌ | Subscription interrupted |
| `Coin Payment/create_receivable` | ❌ | createReceivable failed: { "error": { "tag": "HostFailure", "value": { "reason": "unavailable" } } } |
| `Coin Payment/create_cheque` | ❌ | createCheque failed: { "error": { "tag": "HostFailure", "value": { "reason": "unavailable" } } } |
| `Coin Payment/deposit` | ❌ | Subscription interrupted |
| `Coin Payment/refund` | ❌ | Subscription interrupted |
| `Coin Payment/listen_for_payment` | ❌ | Subscription interrupted |
| `Entropy/derive` | ✅ |  |
| `Local Storage/read` | ✅ |  |
| `Local Storage/write` | ✅ |  |
| `Local Storage/clear` | ✅ |  |
| `Notifications/send_push_notification` | ✅ |  |
| `Notifications/cancel_push_notification` | ✅ |  |
| `Payment/balance_subscribe` | ❌ | Subscription interrupted |
| `Payment/top_up` | ❌ | topUp failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } } } } } |
| `Payment/request` | ❌ | topUp failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } } } } } |
| `Payment/status_subscribe` | ❌ | topUp failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } } } } } |
| `Permissions/request_device_permission` | ✅ |  |
| `Permissions/request_remote_permission` | ✅ |  |
| `Preimage/lookup_subscribe` | ✅ |  |
| `Preimage/submit` | ✅ |  |
| `Resource Allocation/request` | ✅ |  |
| `Signing/create_transaction` | ✅ |  |
| `Signing/create_transaction_with_legacy_account` | ✅ |  |
| `Signing/sign_raw_with_legacy_account` | ✅ |  |
| `Signing/sign_payload_with_legacy_account` | ✅ |  |
| `Signing/sign_raw` | ✅ |  |
| `Signing/sign_payload` | ✅ |  |
| `Statement Store/subscribe` | ✅ |  |
| `Statement Store/create_proof` | ✅ |  |
| `Statement Store/submit` | ✅ |  |
| `Statement Store/create_proof_authorized` | ✅ |  |
| `System/handshake` | ✅ |  |
| `System/feature_supported` | ✅ |  |
| `System/navigate_to` | ✅ |  |
| `Theme/subscribe` | ✅ |  |
