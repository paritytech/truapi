## Truapi Headless Pairing Host Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Account/connection_status_subscribe` | ✅ |  |
| `Account/get_account` | ✅ |  |
| `Account/get_account_alias` | ✅ |  |
| `Account/create_account_proof` | ⏭️ |  |
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
| `Chain/stop_transaction` | ❌ | stopTransaction failed: { "error": { "tag": "HostFailure", "value": { "reason": "remote_chain_transaction_stop: User error: Invalid operation id (-32602)" } } } |
| `Chat/create_room` | ⏭️ |  |
| `Chat/register_bot` | ⏭️ |  |
| `Chat/list_subscribe` | ⏭️ |  |
| `Chat/post_message` | ⏭️ |  |
| `Chat/action_subscribe` | ⏭️ |  |
| `Chat/custom_message_render_subscribe` | ⏭️ |  |
| `Coin Payment/create_purse` | ⏭️ |  |
| `Coin Payment/query_purse` | ⏭️ |  |
| `Coin Payment/rebalance_purse` | ⏭️ |  |
| `Coin Payment/delete_purse` | ⏭️ |  |
| `Coin Payment/create_receivable` | ⏭️ |  |
| `Coin Payment/create_cheque` | ⏭️ |  |
| `Coin Payment/deposit` | ⏭️ |  |
| `Coin Payment/refund` | ⏭️ |  |
| `Coin Payment/listen_for_payment` | ⏭️ |  |
| `Entropy/derive` | ✅ |  |
| `Local Storage/read` | ✅ |  |
| `Local Storage/write` | ✅ |  |
| `Local Storage/clear` | ✅ |  |
| `Notifications/send_push_notification` | ✅ |  |
| `Notifications/cancel_push_notification` | ✅ |  |
| `Payment/balance_subscribe` | ⏭️ |  |
| `Payment/top_up` | ⏭️ |  |
| `Payment/request` | ⏭️ |  |
| `Payment/status_subscribe` | ⏭️ |  |
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
