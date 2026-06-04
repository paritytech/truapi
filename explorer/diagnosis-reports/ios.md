## Truapi iOS Diagnosis

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
| `Chat/create_room` | ❌ | timed out after 10s |
| `Chat/register_bot` | ❌ | registerBot failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Chat/list_subscribe` | ❌ | timed out after 10s |
| `Chat/post_message` | ❌ | postMessage failed: { "error": { "tag": "Unknown", "value": { "reason": "Error: Messages are not supported" } } } |
| `Chat/action_subscribe` | ❌ | timed out after 10s |
| `Chat/custom_message_render_subscribe` | ❌ | timed out after 10s |
| `Entropy/derive` | ✅ |  |
| `Local Storage/read` | ✅ |  |
| `Local Storage/write` | ✅ |  |
| `Local Storage/clear` | ✅ |  |
| `Notifications/send_push_notification` | ✅ |  |
| `Notifications/cancel_push_notification` | ✅ |  |
| `Payment/balance_subscribe` | ✅ |  |
| `Payment/top_up` | ✅ |  |
| `Payment/request` | ✅ |  |
| `Payment/status_subscribe` | ✅ |  |
| `Permissions/request_device_permission` | ✅ |  |
| `Permissions/request_remote_permission` | ✅ |  |
| `Preimage/lookup_subscribe` | ✅ |  |
| `Preimage/submit` | ✅ |  |
| `Resource Allocation/request` | ✅ |  |
| `Signing/create_transaction` | ✅ |  |
| `Signing/create_transaction_with_legacy_account` | ❌ | createTransactionWithLegacyAccount failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Signing/sign_raw_with_legacy_account` | ❌ | signRawWithLegacyAccount failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Signing/sign_payload_with_legacy_account` | ❌ | signPayloadWithLegacyAccount failed: { "error": { "tag": "Unknown", "value": { "reason": "Not implemented" } } } |
| `Signing/sign_raw` | ✅ |  |
| `Signing/sign_payload` | ✅ |  |
| `Statement Store/subscribe` | ❌ | submitting proof: { "tag": "Sr25519", "value": { "signature": "0xfa44b13cbdcab2848cc94f72e272243be38813c027b95117c2ca02f9f4509d45fd5b5c663415f38a861e881ca854f7caa9c8c4a3fcab9a8d4529db01dc40158b", "signer": "0xb2d8f06be3aee758a87b0e2b429df44842463a2a37994d297abcd8a59b18e231" } } proof submitted: timed out after 10s |
| `Statement Store/create_proof` | ✅ |  |
| `Statement Store/submit` | ✅ |  |
| `Statement Store/create_proof_authorized` | ✅ |  |
| `System/handshake` | ✅ |  |
| `System/feature_supported` | ✅ |  |
| `System/navigate_to` | ✅ |  |
| `Theme/subscribe` | ✅ |  |
