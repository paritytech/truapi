## Truapi Desktop Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Account/connection_status_subscribe` | ✅ |  |
| `Account/get_account` | ✅ |  |
| `Account/get_account_alias` | ✅ |  |
| `Account/create_account_proof` | ❌ | createAccountProof failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
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
| `Chat/create_room` | ❌ | createRoom failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Chat/register_bot` | ❌ | registerBot failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Chat/list_subscribe` | ❌ | no elements in sequence |
| `Chat/post_message` | ❌ | postMessage failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Chat/action_subscribe` | ❌ | no elements in sequence |
| `Chat/custom_message_render_subscribe` | ❌ | timed out after 10s |
| `Entropy/derive` | ✅ |  |
| `Local Storage/read` | ✅ |  |
| `Local Storage/write` | ✅ |  |
| `Local Storage/clear` | ✅ |  |
| `Notifications/send_push_notification` | ✅ |  |
| `Notifications/cancel_push_notification` | ✅ |  |
| `Payment/balance_subscribe` | ❌ | Subscription interrupted |
| `Payment/top_up` | ❌ | topUp failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Payment/request` | ❌ | topUp failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Payment/status_subscribe` | ❌ | topUp failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Permissions/request_device_permission` | ✅ |  |
| `Permissions/request_remote_permission` | ✅ |  |
| `Preimage/lookup_subscribe` | ✅ |  |
| `Preimage/submit` | ✅ |  |
| `Resource Allocation/request` | ✅ |  |
| `Signing/create_transaction` | ✅ |  |
| `Signing/create_transaction_with_legacy_account` | ❌ | createTransactionWithLegacyAccount failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Signing/sign_raw_with_legacy_account` | ❌ | signRawWithLegacyAccount failed: { "error": { "tag": "Unknown", "value": { "reason": "Account can't be derived from product account id" } } } |
| `Signing/sign_payload_with_legacy_account` | ❌ | signPayloadWithLegacyAccount failed: { "error": { "tag": "Unknown", "value": { "reason": "Account can't be derived from product account id" } } } |
| `Signing/sign_raw` | ✅ |  |
| `Signing/sign_payload` | ✅ |  |
| `Statement Store/subscribe` | ✅ |  |
| `Statement Store/create_proof` | ✅ |  |
| `Statement Store/submit` | ✅ |  |
| `Statement Store/create_proof_authorized` | ✅ |  |
| `System/handshake` | ✅ |  |
| `System/feature_supported` | ✅ |  |
| `System/navigate_to` | ✅ |  |
| `Theme/subscribe` | ❌ | Offset is outside the bounds of the DataView |
