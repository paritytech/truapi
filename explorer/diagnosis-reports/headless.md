## Truapi Headless Pairing Host Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Account/connection_status_subscribe` | ✅ | connection status: Connected |
| `Account/get_account` | ✅ | account retrieved: { "account": { "publicKey": "0xf41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13" } } |
| `Account/get_account_alias` | ✅ | account alias: { "context": "0x05cc3451e5a525ff21f0a62ffe5e5c184fa4cd790bbaead4ef44ab8dc914ecee", "alias": "0x34cc10180a6fc6977f179b03ae39a261ae5cd44fab26d2086545e00b1b94368c" } |
| `Account/create_account_proof` | ⏭️ |  |
| `Account/get_legacy_accounts` | ✅ | legacy accounts: { "accounts": [ { "publicKey": "0xf41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13", "name": "pgherveou.06" } ] } |
| `Account/get_user_id` | ✅ | user id: { "primaryUsername": "pgherveou.06" } |
| `Account/request_login` | ✅ | login completed: AlreadyConnected |
| `Chain/follow_head_subscribe` | ✅ | head follow event: { "tag": "Initialized", "value": { "finalizedBlockHashes": [ "0xa5ae138f1b4cd068bc704ee3cb6f9358b21e82799451d1b08658be523757fe2f", "0x2c2ec575d2c336b34f635965ef2636587938fda8d64578fbf88f720947a1203a", "0xd8abfa22d8ed08c4728916457d353f9b122c5f109bef8f8727d956b4ad1e37b5", "0x541e... |
| `Chain/get_head_header` | ✅ | block header: { "header": "0xf9a495e0317aa41a3e1d51e73ecb9a9d4a495ed4e0991f838207944dc521a0e7aaba5a006d711144eeb697c01f656c61e86a9747f958e10c9f9037fd1dc52df3c0104ba52b0f9b97f61207308e3a1b3f5ba6616de4073a48c3279956d1b8f64340d7e6701006434d4c531001000104066175726120a98edb08000000000452505352903d2a29... |
| `Chain/get_head_body` | ✅ | block body: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/get_head_storage` | ✅ | storage value: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/call_head` | ✅ | runtime call result: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/unpin_head` | ✅ | blocks unpinned |
| `Chain/continue_head` | ✅ | operation continued |
| `Chain/stop_head_operation` | ✅ | operation stopped |
| `Chain/get_spec_genesis_hash` | ✅ | genesis hash: { "genesisHash": "0xbf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f" } |
| `Chain/get_spec_chain_name` | ✅ | chain name: { "chainName": "Paseo Asset Hub Next" } |
| `Chain/get_spec_properties` | ✅ | chain properties: { "properties": "{\"tokenDecimals\":10,\"tokenSymbol\":\"PAS\"}" } |
| `Chain/broadcast_transaction` | ✅ | transaction broadcast: { "operationId": "IOSWFVOvUOCr6EqA" } |
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
| `Entropy/derive` | ✅ | entropy derived: { "entropy": "0x9ad6f1f6ac64687863a3456a7ddcb06a94c0b9a950930bb8eea3b51743f70baa" } |
| `Local Storage/read` | ✅ | storage value read: |
| `Local Storage/write` | ✅ | storage write succeeded |
| `Local Storage/clear` | ✅ | storage clear succeeded |
| `Notifications/send_push_notification` | ✅ | notification sent: { "id": 1 } |
| `Notifications/cancel_push_notification` | ✅ | notification cancelled |
| `Payment/balance_subscribe` | ⏭️ |  |
| `Payment/top_up` | ⏭️ |  |
| `Payment/request` | ⏭️ |  |
| `Payment/status_subscribe` | ⏭️ |  |
| `Permissions/request_device_permission` | ✅ | device permission result: { "granted": true } |
| `Permissions/request_remote_permission` | ✅ | remote permission result: { "granted": true } |
| `Preimage/lookup_subscribe` | ✅ | preimage lookup received: { "value": "0xdeadbeef" } |
| `Preimage/submit` | ✅ | preimage submitted: 0xf3e925002fed7cc0ded46842569eb5c90c910c091d8d04a1bdf96e0db719fd91 |
| `Resource Allocation/request` | ✅ | resource allocation result: { "outcomes": [ "NotAvailable", "NotAvailable" ] } |
| `Signing/create_transaction` | ✅ | transaction created: { "transaction": "0xd9018400f41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef1301a277fa7a08aaa6253478e7f5a974ccde4b4a89aa891d94150815d3a514a01e1a7bd5d16fbc3e8df09f39d112dc92dbf2854b10dc4bca2d5bff21aa966609008400000000000000000000000000000000000000" } |
| `Signing/create_transaction_with_legacy_account` | ✅ | selected legacy account: { "publicKey": "0xf41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13", "name": "pgherveou.06" } transaction created: { "transaction": "0xd9018400f41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef1301cea6ac96e57e11d4f1f910704963723e1e18df71947f2bf... |
| `Signing/sign_raw_with_legacy_account` | ✅ | raw bytes signed: { "signature": "0x5e3a2c277cfab5b897be4e49b2e01d31297d40da7e4dead2a14b42a17f38c27cb836c8f30985cac947cf656c7d27aa808e2d827f132d57e101b61241d5c74c8b" } |
| `Signing/sign_payload_with_legacy_account` | ✅ | payload signed: { "signature": "0xd0e90abc571c107f492f3d6b8b0811799e3869d12e0b4a103c2a77b81cf03034079441f76884c823a14a6577b0f12e018608735e12c149cddc296fe851b19a87" } |
| `Signing/sign_raw` | ✅ | raw bytes signed: { "signature": "0x641d998ce2ce375ab55682a22d2e941bb881f99ed986aa44aa1cbc5d23734569667f098642436d72d44fe199be2c9d062ca992a7e8578cb2627b04111d081b87" } |
| `Signing/sign_payload` | ✅ | payload signed: { "signature": "0xf6a22196b0ad973c405ab42a0041b3ac83fed630c0257663707ebc593dcae179189aaa6e3db2bc3f4ba8e52d99bfc76f6ec54d8f66923d5fb5d236fa02b4a483" } |
| `Statement Store/subscribe` | ✅ | submitting statement: { "expiry": "7659500274590941184n", "topics": [ "0xa54edfe3483582af1d5eca2fe801f0dce1737aaa01805c28992b1feda9f424ea" ], "proof": { "tag": "Sr25519", "value": { "signature": "0x683790faacf0cdd69fd7ddbe467487a99b6b745ed9b2de14e4dc5a94b2c8875ec9bc1c4da29d8791ea59939a1b56c4b831e... |
| `Statement Store/create_proof` | ✅ | proof created: { "proof": { "tag": "Sr25519", "value": { "signature": "0x740f4a72c94a4fe3a10b8bb2c1569dadf2e7b9970eb79fc17767e068b219616c16e8852ce1afac38d544b6f0cf4fe3964f0460601963376aeb40e2b988a64c8d", "signer": "0x92efa72e75d92c115fb4d596387290e4409ff1b1a72a3ec39457ef351086d838" } } } |
| `Statement Store/submit` | ✅ | statement submitted |
| `Statement Store/create_proof_authorized` | ✅ | proof created: { "proof": { "tag": "Sr25519", "value": { "signature": "0x1ee8ba4f76b76029dd398f9f1c364aeb25fcb8c97a9ae0d2cec5a94527c07d35122c37b12f7c92bab87ff52bdc1acee73927f7b93859554c7cc8b98a122b6c83", "signer": "0x92efa72e75d92c115fb4d596387290e4409ff1b1a72a3ec39457ef351086d838" } } } |
| `System/handshake` | ✅ | handshake succeeded |
| `System/feature_supported` | ✅ | feature supported: true |
| `System/navigate_to` | ✅ | navigation succeeded |
| `Theme/subscribe` | ✅ | theme received: { "name": { "tag": "Default" }, "variant": "Dark" } |
