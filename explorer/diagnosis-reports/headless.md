## Truapi Headless Pairing Host Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Account/connection_status_subscribe` | ✅ | connection status: Connected |
| `Account/get_account` | ✅ | account retrieved: { "account": { "publicKey": "0xf41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13" } } other product account retrieved after approval: { "account": { "publicKey": "0x54931f317cc49b2448a7fd1c1bc816b038065eac9c19a8c53fe6cff21419560d" } } |
| `Account/get_account_alias` | ✅ | account alias: { "context": "0x05cc3451e5a525ff21f0a62ffe5e5c184fa4cd790bbaead4ef44ab8dc914ecee", "alias": "0x34cc10180a6fc6977f179b03ae39a261ae5cd44fab26d2086545e00b1b94368c" } |
| `Account/create_account_proof` | ⏭️ |  |
| `Account/get_legacy_accounts` | ✅ | legacy accounts: { "accounts": [ { "publicKey": "0xf41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13", "name": "pgherveou.06" } ] } |
| `Account/get_user_id` | ✅ | user id: { "primaryUsername": "pgherveouu" } |
| `Account/request_login` | ✅ | login completed: AlreadyConnected |
| `Chain/follow_head_subscribe` | ✅ | head follow event: { "tag": "Initialized", "value": { "finalizedBlockHashes": [ "0x24a8038e7cc8a27d6fec2ae9b77b16785a1d1c794c6a365fac0741c352262364", "0x46c922275f522cf267838ffefde7b9a06390e798c306cb84c866e5bde6d55c4c", "0xdc7034d48de1f314b6bff28cd8c20600f45426188720d99bba5a058a63a2cb03", "0x43c1... |
| `Chain/get_head_header` | ✅ | block header: { "header": "0xd7a5dcde73a638f6cf32ddf9742e8ed344f3d74a60238baee743f2afdd6a138d22506200d0ed8a003223ddb768e65599cb3c356a59f42b2cf9c705dab8fff471b7fe013837993da375488a851df8efd0f70c3ea683246f565d680046a0feddecb481c37c1006434d4c53100101010c0661757261202930dc0800000000045250535290018a90... |
| `Chain/get_head_body` | ✅ | block body: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/get_head_storage` | ✅ | storage value: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/call_head` | ✅ | runtime call result: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/unpin_head` | ✅ | blocks unpinned |
| `Chain/continue_head` | ✅ | operation continued |
| `Chain/stop_head_operation` | ✅ | operation stopped |
| `Chain/get_spec_genesis_hash` | ✅ | genesis hash: { "genesisHash": "0xbf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f" } |
| `Chain/get_spec_chain_name` | ✅ | chain name: { "chainName": "Paseo Asset Hub Next" } |
| `Chain/get_spec_properties` | ✅ | chain properties: { "properties": "{\"tokenDecimals\":10,\"tokenSymbol\":\"PAS\"}" } |
| `Chain/broadcast_transaction` | ✅ | transaction broadcast: { "operationId": "kVxLWAWJZIuYd3KM" } |
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
| `Preimage/lookup_subscribe` | ❌ | submit failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "bulletin allowance is not available" } } } } } |
| `Preimage/submit` | ❌ | submit failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "bulletin allowance is not available" } } } } } |
| `Resource Allocation/request` | ✅ | resource allocation result: { "outcomes": [ "Allocated", "NotAvailable" ] } |
| `Signing/create_transaction` | ✅ | transaction created: { "transaction": "0xd9018400f41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef1301ae9c35d29363cd548f24c435c1a71af4971b119f32e376af9dc15e658d29c70e65baa9dd8430ecb7eee566f821ab87aba43d39821174de067478e45dff81c58a00000000000000000000000000000000000000" } |
| `Signing/create_transaction_with_legacy_account` | ✅ | selected legacy account: { "publicKey": "0xf41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13", "name": "pgherveou.06" } transaction created: { "transaction": "0xd9018400f41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13012430b825adcbb1321b1310144a110809ae758cbc66b94d0... |
| `Signing/sign_raw_with_legacy_account` | ✅ | raw bytes signed: { "signature": "0x3af5d72c406e9279316f5ee707f4c320717ec72a55eb1ab80a04c36708079b3c486fc9102153c0dc5093d2630974302dc1ad6d129fe7024a137836a7331b8980" } |
| `Signing/sign_payload_with_legacy_account` | ✅ | payload signed: { "signature": "0x0061084213f1c30f5776f34ae6a84b14d5b1c3fc6ffca3b531986ae5456be523462d23f3da45e4f8c389c0bd55af3c3099be25da32bce68a8c5d605ccc1a4086" } |
| `Signing/sign_raw` | ✅ | raw bytes signed: { "signature": "0xd0afe46e7df6c2e032c3cb2fd923f96519aee31dda922770beba20c9d09e66389307bb1da61c53ea394e6babee235c852c1cd1a4af1d208153cdc06c767e2686" } |
| `Signing/sign_payload` | ✅ | payload signed: { "signature": "0x1eb6986df46f3347fb3054f8f2864f40eac4f03d43a05b2529a5b8e30fed1b4591da6ea79da89baf056bf4e94071d635ae0c32c5554d899d77cbe7e6a8d73986" } |
| `Statement Store/subscribe` | ✅ | submitting statement: { "expiry": "7661629886880022528n", "topics": [ "0xa962ad4e75e2fc9251c2fb585f98c83a69db2ce0cb99bbb7e0714591bdd9b1a6" ], "proof": { "tag": "Sr25519", "value": { "signature": "0xe6ec7fcffd0322d0bedc33ef99a491d1c6fd5a29b197d337e9bbb984309e544260098b3954cc936628f5deb82c51f5a95cc... |
| `Statement Store/create_proof` | ✅ | proof created: { "proof": { "tag": "Sr25519", "value": { "signature": "0x92b74641cd174206267af9a8bf11d09a20ab0a37924acc2018c386586b666f1e168831f7e126da6a139b7ef0809b2c286c7e89d4d7c5ff1cc800286f1b819486", "signer": "0xf41df7581a7a4bdf59ab2497f3d86dd8ea14db35c5c1930790d5a3554d0bef13" } } } |
| `Statement Store/submit` | ✅ | statement submitted |
| `Statement Store/create_proof_authorized` | ✅ | proof created: { "proof": { "tag": "Sr25519", "value": { "signature": "0x0e5ada3138607a38cf64e894cf50ccb2523b0de19a77f8bd7f984c39e6d71f38bc46b2fc7d2215610039f211f31e01bbf78b19d4caf25d7a8b481f2148bd618c", "signer": "0x60c724a9f56cb147403ae2ac1e7b55170ec8c29e127edc72dfb239431b03a379" } } } |
| `System/handshake` | ✅ | handshake succeeded |
| `System/feature_supported` | ✅ | feature supported: true |
| `System/navigate_to` | ✅ | navigation succeeded |
| `Theme/subscribe` | ✅ | theme received: { "name": { "tag": "Default" }, "variant": "Dark" } |
