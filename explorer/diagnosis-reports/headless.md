## Truapi Headless Pairing Host Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Account/connection_status_subscribe` | ✅ | connection status: Connected |
| `Account/get_account` | ✅ | account retrieved: { "account": { "publicKey": "0xfca633ac856b1e3eaddd5fa1e82baece58fbea027e3f5ff3321e5ecdec8c0a38" } } |
| `Account/get_account_alias` | ✅ | account alias: { "context": "0x05cc3451e5a525ff21f0a62ffe5e5c184fa4cd790bbaead4ef44ab8dc914ecee", "alias": "0xb032abb7574e4f2fa7895896586f14b4189312afd59a6a006564ae9787594834" } |
| `Account/create_account_proof` | ⏭️ |  |
| `Account/get_legacy_accounts` | ✅ | legacy accounts: { "accounts": [ { "publicKey": "0xfca633ac856b1e3eaddd5fa1e82baece58fbea027e3f5ff3321e5ecdec8c0a38", "name": "guestnrun.93" } ] } |
| `Account/get_user_id` | ✅ | user id: { "primaryUsername": "guestnrun.93" } |
| `Account/request_login` | ✅ | login completed: AlreadyConnected |
| `Chain/follow_head_subscribe` | ✅ | head follow event: { "tag": "Initialized", "value": { "finalizedBlockHashes": [ "0x576535cc60a3538ced094ff3e0cd40e69441918f0baf501b18465e333d82d25e", "0x6ae85468b9062598632eef6ac0b6748b0ffbf2f1a764b50870e7627d111e27e2", "0x5b49c32ec0f0f7aa493c09e742850421b0b2e4069be256c8c27d1dfb687413cf", "0xaeb7... |
| `Chain/get_head_header` | ✅ | block header: { "header": "0x1fb8cfd495dcc15812e72a6d16c9c1803c54bd9bdb179198ee49202840bb73b332db5a009a4e087e9aff10872c94b0841cc799411c3e545102217aa65db6b86980a056879b56d1cdf162436c88f29877570c007b5064af3ff4414380f167cf4af705b9ac1006434d4c53100100010406617572612050a0db080000000004525053529076047e... |
| `Chain/get_head_body` | ✅ | block body: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/get_head_storage` | ✅ | storage value: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/call_head` | ✅ | runtime call result: { "operation": { "tag": "Started", "value": { "operationId": "0" } } } |
| `Chain/unpin_head` | ✅ | blocks unpinned |
| `Chain/continue_head` | ✅ | operation continued |
| `Chain/stop_head_operation` | ✅ | operation stopped |
| `Chain/get_spec_genesis_hash` | ✅ | genesis hash: { "genesisHash": "0xbf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f" } |
| `Chain/get_spec_chain_name` | ✅ | chain name: { "chainName": "Paseo Asset Hub Next" } |
| `Chain/get_spec_properties` | ✅ | chain properties: { "properties": "{\"tokenDecimals\":10,\"tokenSymbol\":\"PAS\"}" } |
| `Chain/broadcast_transaction` | ✅ | transaction broadcast: { "operationId": "FCGwt6VNmp37UdU1" } |
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
| `Entropy/derive` | ✅ | entropy derived: { "entropy": "0x38bb56cd04aed300dfb618c03016c9f1ec8304617d32b54d30bbbcf27528ede1" } |
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
| `Signing/create_transaction` | ✅ | transaction created: { "transaction": "0xd9018400fca633ac856b1e3eaddd5fa1e82baece58fbea027e3f5ff3321e5ecdec8c0a38010234909e12d9c3c4522b9527d1d8315082409a42f46c748e2523cb2a45d805709c266ec8367aed7f6976e1275404039f256427fb5b8962fbb11f8b21c601778500000000000000000000000000000000000000" } |
| `Signing/create_transaction_with_legacy_account` | ✅ | selected legacy account: { "publicKey": "0xfca633ac856b1e3eaddd5fa1e82baece58fbea027e3f5ff3321e5ecdec8c0a38", "name": "guestnrun.93" } transaction created: { "transaction": "0xd9018400fca633ac856b1e3eaddd5fa1e82baece58fbea027e3f5ff3321e5ecdec8c0a38010e8284365affca179e95690f753ef80d0b245930ce29edc... |
| `Signing/sign_raw_with_legacy_account` | ✅ | raw bytes signed: { "signature": "0x00ac3a33bda4ac91500717f1a71c5529462e04c2d07411b96b79c064b764c3644febadac7e373973798a03fb95d9e96eb5262017ddc991b7ff6badd77e25a48b" } |
| `Signing/sign_payload_with_legacy_account` | ✅ | payload signed: { "signature": "0xec1558e930e22555fd4e6635f8ee2d91be02303ec0fd2713e2c6568bc67c1c6e2e55784da9893ed467b739ce3f38cd45b5fe0995f189833dfa02b738ecdc278a" } |
| `Signing/sign_raw` | ✅ | raw bytes signed: { "signature": "0xc6f96f3e49630290fb2141d8bc157b9bc1b6734ec90cca99bbbabdaf74eb42704e7449c13830c0afe28f7e395f7bd4b3019002653bfa437fdc5c99cae98a5988" } |
| `Signing/sign_payload` | ✅ | payload signed: { "signature": "0x2a7000ea648925fe611012ab0eda862a313ecce9e478b9ea2087143722979e27efd5e59b8a619f0fccbe77c15280f2e3e710bf745607b72fd945b70855fb0b8f" } |
| `Statement Store/subscribe` | ✅ | submitting statement: { "expiry": "7659733435480539136n", "topics": [ "0xb00a99642d2f4b7d2928e8d71d6a1147112be506053c72653ad710bd708a1ac5" ], "proof": { "tag": "Sr25519", "value": { "signature": "0x18201b3dbae5222f50035bf25161f3e4f58c68b3a7904d352b26fa303fa4c54163980a13581bb5e7499c353aaa0d35fdc1b... |
| `Statement Store/create_proof` | ✅ | proof created: { "proof": { "tag": "Sr25519", "value": { "signature": "0xa864c5c0bd2fe22dcb871df0e52cd612a611d60f416894670153a39fd714aa786915a4f551780f193c215baaec4969ac1978c24ff7b848e639240c118da38587", "signer": "0xd6dc1d8edb8088d7b489c7cffe4062fbedc5e90b43e40a576b5212b4a6b6ca1d" } } } |
| `Statement Store/submit` | ✅ | statement submitted |
| `Statement Store/create_proof_authorized` | ✅ | proof created: { "proof": { "tag": "Sr25519", "value": { "signature": "0x4017df05ca60010b0d7656b925f5c6e397dfd8cf720a0ce92d1d38aa452cab169a970430c5fba68a1321f750b0eb138fdb256e0e4c06576dd2acee398165e387", "signer": "0xd6dc1d8edb8088d7b489c7cffe4062fbedc5e90b43e40a576b5212b4a6b6ca1d" } } } |
| `System/handshake` | ✅ | handshake succeeded |
| `System/feature_supported` | ✅ | feature supported: true |
| `System/navigate_to` | ✅ | navigation succeeded |
| `Theme/subscribe` | ✅ | theme received: { "name": { "tag": "Default" }, "variant": "Dark" } |
