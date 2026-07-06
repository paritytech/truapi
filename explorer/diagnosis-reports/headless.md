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
| `Chain/follow_head_subscribe` | ✅ | head follow event: { "tag": "Initialized", "value": { "finalizedBlockHashes": [ "0x250744308f9f54705da08b8ed1360756a60c4c4d5ed0dd028f6d3ffaef34697c", "0x167e135c78af237da48362dce50efdd1a9362c0248250935890f060b9ac2572e", "0xb772c6963007376a073bd79bbb0ede819a97ab3605a26975df75be98116ffb6f", "0xc0cb... |
| `Chain/get_head_header` | ✅ | block header: { "header": "0x4a65540ed66cc5b8a4023520d76a617c1cf95ce8ddb9ccd86890e7c26a45dd0616d05a0000dc38d0b9c4aad4dd573ababe1457db2ee8455f4a4107cbaed510a67027050538cc0edf23a8f6548e4d80afe3f1f2daa9f3dd50b732f86a8cca75bf5fb512331006434d4c531001000104066175726120479adb0800000000045250535290f2cf90... |
| `Chain/get_head_body` | ✅ | block body: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/get_head_storage` | ✅ | storage value: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/call_head` | ✅ | runtime call result: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/unpin_head` | ✅ | blocks unpinned |
| `Chain/continue_head` | ✅ | operation continued |
| `Chain/stop_head_operation` | ✅ | operation stopped |
| `Chain/get_spec_genesis_hash` | ✅ | genesis hash: { "genesisHash": "0xbf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f" } |
| `Chain/get_spec_chain_name` | ✅ | chain name: { "chainName": "Paseo Asset Hub Next" } |
| `Chain/get_spec_properties` | ✅ | chain properties: { "properties": "{\"tokenDecimals\":10,\"tokenSymbol\":\"PAS\"}" } |
| `Chain/broadcast_transaction` | ✅ | transaction broadcast: { "operationId": "78Z1IqDRkuXpP8rO" } |
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
| `Signing/create_transaction` | ✅ | transaction created: { "transaction": "0xd9018400f41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef1301c8264dd11330720ff8232378c87186794c5e577abdd74d74cd967367f623e30e1e14165e7cdec32a7f9b566faf06e026fa7d71cf6f510062094f284293daf08d00000000000000000000000000000000000000" } |
| `Signing/create_transaction_with_legacy_account` | ✅ | selected legacy account: { "publicKey": "0xf41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13", "name": "pgherveou.06" } transaction created: { "transaction": "0xd9018400f41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef1301c2a7dfb92f096c1bfc1d680f234ae203e68352dd9e79dff... |
| `Signing/sign_raw_with_legacy_account` | ✅ | raw bytes signed: { "signature": "0xd291fcd096acd68f0fdbdada30ac91bc58bd7960e9328ccb96b251effc991c7518cabe8c9559a33426b2da0a6beec409980bdfb91502a2fa84ee889624236681" } |
| `Signing/sign_payload_with_legacy_account` | ✅ | payload signed: { "signature": "0x70636930d6fbdeff4587ea2500def78fefe75195ad0e5ec09e8e945fa82af22b24b986ed01a0de3c2b270d9bdf623e1abb53836b491163925b71ffc9205d628e" } |
| `Signing/sign_raw` | ✅ | raw bytes signed: { "signature": "0xfcc96443f18f6e8b1340fdfaa3baa5d33f8f983014a9ce602ede6e7a769067569f32514e95a481bbcc5a0a5c1c02a175458596bd436fea50c5c9d870e963ba8a" } |
| `Signing/sign_payload` | ✅ | payload signed: { "signature": "0x625e911b25ca6471b5aa4b917012ccbad10a54347142e78a31930901749480457a7fee72d0b9a8ab666b948de6db9b094a09a4f0508e9c666689c0a8a5e7d189" } |
| `Statement Store/subscribe` | ✅ | submitting statement: { "expiry": "7659654034420137984n", "topics": [ "0xd61590958f674d52bad9c60348459ce813540d1a5108266f803d3083530dafaf" ], "proof": { "tag": "Sr25519", "value": { "signature": "0x0a3feb7cebe294484fbe3beda1a7474cec09cbc1f08dfbe31f353b2497cfb9184adae85612ab6c0ae5e1e30c76142d36ec8... |
| `Statement Store/create_proof` | ✅ | proof created: { "proof": { "tag": "Sr25519", "value": { "signature": "0x2299d6c886a51b79f8de93e15269f5dc5601f0a379ce19243e8366db93e98b46f1a3caed8cf04265752d086a684afb50c7a1348af31e91f0cbb73246db485186", "signer": "0xd6af71ea43ef71caea4afb37d908333411a79aad6f2559864296606d47d47a52" } } } |
| `Statement Store/submit` | ✅ | statement submitted |
| `Statement Store/create_proof_authorized` | ✅ | proof created: { "proof": { "tag": "Sr25519", "value": { "signature": "0xd861edf3ff18bdb04c0f6d76add5429014afa964521740147044ce7ac2d9d3369446a8c888b4efef32269dac45ad6d7bf07e34f278037ddb7ee2f3bd60304786", "signer": "0xd6af71ea43ef71caea4afb37d908333411a79aad6f2559864296606d47d47a52" } } } |
| `System/handshake` | ✅ | handshake succeeded |
| `System/feature_supported` | ✅ | feature supported: true |
| `System/navigate_to` | ✅ | navigation succeeded |
| `Theme/subscribe` | ✅ | theme received: { "name": { "tag": "Default" }, "variant": "Dark" } |
