# H - SSO pairing & message-exchange protocol

> Part of the [host-contract & core-impl spec](index.md). Foundational: `request_login`, signing,
> transaction construction, resource allocation, ring-VRF alias, and the product statement store all ride
> this one mechanism. Read this before
> [A](<A - host-primitives.md>) / [B](<B - core-impls.md>).
>
> Source of truth for the current protocol is dotli main's pinned
> `@novasamatech/host-papp@0.8.6` / `@novasamatech/statement-store@0.8.6` web
> code plus the iOS wallet. Current-dotli parity uses SSO V2.
> (`~/github/polkadot-app-ios-v2`), the peer that implements the other side in readable Swift. We define
> the **web/core** side.

## Why (before the what)

A sandboxed web product must act for a user whose signing keys live in a phone wallet, with **no shared
backend** between them. The solution: both sides rendezvous on the **People-chain statement store** (a
gossip pallet that distributes signed SCALE `Statement`s P2P) and talk over it with an
application-layer-encrypted channel. A QR code bootstraps the rendezvous.

```
   web product  <--- encrypted SCALE statements on derived topics --->  phone wallet
                     (People-chain statement-store pallet, P2P gossip)
                            ^ no relay server, no shared backend
```

Three consequences shape the core API:

1. **There is exactly one transport.** Pairing, transaction/raw signing, ring-VRF alias, resource
   allocation, and the product-facing statement store are all carried as statements on the same
   People-chain connection. The core reaches it through the **existing `ChainProvider`** platform trait
   (connect to People-chain, then `statement_submit` / `statement_subscribeStatement` JSON-RPC). No new
   transport primitive is needed.
2. **The wallet keeps every signing key and only ever signs on request.** During the handshake it sends
   _public_ keys + account ids, never a secret (confirmed in the iOS source). Transaction signing and
   ring-VRF run as request/response messages over the channel: the core asks, the wallet signs, the
   wallet replies. So signing is **not** a host primitive that "reaches the wallet"; it is core protocol.
3. **It is a cryptographic protocol, so the core owns it.** Challenge keys, ECDH, channel encryption,
   topic derivation, and proof verification are protocol logic. The host's only job is to **render the
   QR** ([A1](<A - host-primitives.md>)) and provide the chain connection (`ChainProvider`). The core
   cannot be lied to about identity because it verifies the statements itself.

Net effect on the SSO host contract: the only new protocol primitive is the QR presenter. `host-papp` is
fully replaceable by core message-exchange logic, which resolves [E2](<E - open-questions.md>). Separate
host-container parity surfaces such as notifications, theme, and preimage are covered in
[A3](<A - host-primitives.md>).

## What the core holds vs. what the wallet holds

| Generated/held by the **core** (web side)                                                                            | Generated/held by the **wallet**                                                                          |
| -------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| session **sr25519 statement-store keypair** (signs all the core's outgoing statements; its secret is the `ssSecret`) | root + identity **sr25519 keypairs** (sign transactions; never leave the device)                          |
| **P-256 (secp256r1) ECDH keypair** (channel encryption; `encryptionPublicKey` = X9.63 uncompressed, 65 bytes)        | persistent **P-256 chat keypair** (`sharedSecretDerivationKey`, the public half is sent in the handshake) |
| derived session ids + channel ids; the peer's public keys + account ids                                              | the ring (bandersnatch) secret (ring-VRF alias derivation)                                                |

The wallet grants the core's statement-store account **allowance** to submit (a `resourceAllocation`
message), which is why the core can submit product statement proofs signed by its own session key.

## The protocol

### Statement wire format (People-chain statement store)

A `Statement` is a SCALE struct of indexed `StatementField`s, sorted by index before signing:
`proof`(0) = `Sr25519 { signature:[u8;64], signer:[u8;32] }`, `expiry`(2):u64, `channel`(3):[u8;32],
`topic1..4`(4..7):[u8;32], `scaleEncodedPayload`(8):bytes. The `proof` is an sr25519 signature over the
SCALE of every field except the proof; `signer` is the submitter's 32-byte sr25519 public key. `expiry`
doubles as priority (`0xFFFF_FFFF_0000_0000 | (unixSecs - EPOCH)`, monotonic) so a newer statement
replaces an older one on the same topic+channel. RPC: `statement_submit(hex)` /
`statement_subscribeStatement({topic_filter:{matchAll:[topic]}})` / `statement_unsubscribeStatement`.

### 1. Handshake bootstrap (the QR)

The core mints a session statement-store sr25519 keypair and a P-256 ECDH keypair, reads the static
pairing runtime config ([A2](<A - host-primitives.md>)), then builds the deeplink the QR encodes:

```
VersionedHandshakeProposal::V2 {   # SCALE enum, index 1
    device: {
        statement_account_id: [u8;32]     # core session sr25519 account id
        encryption_public_key: [u8;65]    # core P-256 pubkey, X9.63 uncompressed
    }
    metadata: Vec<(MetadataKey, String)>  # HostName required; HostVersion, HostIcon,
                                          # PlatformType, PlatformVersion optional
}
deeplink = runtime_config.scheme + "pair?handshake=" + hex(SCALE(VersionedHandshakeProposal))
```

The bootstrap topic is keyed BLAKE2b-256 with key = the core's statement-store pubkey. The V2 channel
uses the same inputs with suffix `"channel"`; the Rust request-login path currently subscribes by topic and
tracks the remote subscription slot:

```
topic = blake2b_256_keyed(key = ss_pubkey, msg = enc_pubkey ++ b"topic")
channel = blake2b_256_keyed(key = ss_pubkey, msg = enc_pubkey ++ b"channel")
```

The core renders the QR via [A1 `PairingPresenter`](<A - host-primitives.md>) and subscribes to `topic`.

### 2. Wallet answer

The wallet scans, decodes `VersionedHandshakeProposal::V2`, displays the embedded metadata, and submits
V2 responses to `(topic, channel)`, signed by its sr25519 statement account. Pending status can arrive
before success while allowance allocation is in progress:

```
VersionedHandshakeResponse::V2 {   # SCALE enum, index 1
    encrypted_message: bytes       # AES-GCM(ephemeral ECDH) of EncryptedHandshakeResponseV2
    public_key:        [u8;65]     # the wallet's ephemeral P-256 pubkey (X9.63)
}

EncryptedHandshakeResponseV2 =
    Pending(AllowanceAllocation)
  | Success {
        identity_account_id:        [u8;32]
        root_account_id:            [u8;32]  # product-account derivation root
        identity_chat_private_key:  [u8;32]
        sso_enc_pub_key:            [u8;65]  # wallet persistent P-256 pubkey
        device_enc_pub_key:         [u8;65]
        root_entropy_source:        [u8;32]
    }
  | Failed(String)
}
```

The core receives the statement (verifying its sr25519 proof), runs ECDH(`core_enc_priv`,
`ephemeral_pubkey`) -> HKDF-SHA256 -> AES-GCM to decrypt. It skips `Pending`, maps `Failed(reason)` to the
login error path, and on `Success` stores the wallet's persistent SSO P-256 key, root account id,
identity account id, and root entropy source. The current web construction is in [D4](<D - crypto-foundation.md>): ECDH uses the
X coordinate from P-256 `getSharedSecret`, HKDF-SHA256 uses empty `salt`/`info` and `len=32`, and
AES-GCM prepends a 12-byte nonce to ciphertext+tag.

### 3. Session establishment

A steady-state shared secret is derived by ECDH between the persistent P-256 keys
(`core_enc_priv` x `sso_enc_pub_key`). From it, directional 32-byte session ids and the channel ids:

```
session_id.own  = H(key = shared_secret, msg = b"session" ++ own_acct ++ peer_acct ++ pin(own) ++ pin(peer))
session_id.peer = H(key = shared_secret, msg = b"session" ++ peer_acct ++ own_acct ++ pin(peer) ++ pin(own))
request_channel  = H(key = session_id.own,  msg = b"request")    # core -> wallet
response_channel = H(key = session_id.own,  msg = b"response")   # wallet ACK / response to core request
peer_request_ch  = H(key = session_id.peer, msg = b"request")    # wallet -> core (filter)
```

`H` is keyed BLAKE2b-256 (`blake2b(message, { dkLen: 32, key })`). A missing pin contributes the
separator string `"/"` in current dotli's `createSessionId`.

The core subscribes `topic1 == session_id.peer` (filtered to the wallet's account id as the only valid
signer) for inbound, and submits with `topic1 == session_id.own`.

### 4. Encrypted message framing

Steady-state messages are AES-GCM encrypted under the session key and wrapped:

```
inner   = SCALE(StatementData)                 # enum: 0=request{request_id:String, messages:[Msg]}, 1=response{request_id:String, code:u8}
sealed  = AES_GCM_seal(inner)                  # = nonce(12) ++ ciphertext ++ tag(16)
payload = SCALE(sealed as bytes)               # length-prefixed -> the statement's scaleEncodedPayload(8)
```

The application messages are `PolkadotHostRemoteMessage { message_id: String, content: V1(ContentV1) }`
where `ContentV1` is a SCALE-tagged enum: `disconnected`(0), `signingRequest`(1)/`signingResponse`(2),
`aliasRequest`(3)/`aliasResponse`(4), `resourceAllocationRequest`(5)/`response`(6),
`createTransactionRequest`(7)/`response`(8). Responses carry `request_message_id` + `HostResult<T>`
(`success(T)`=0 / `failure(String)`=1).

### 5. Operations after pairing (mapped to wire methods)

| Wire method                                                         | Message                                                                                                     | Notes                                                                                                                                                                                                                                                                          |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `sign_payload`(116)                                                 | `signingRequest(transaction(SignTransactionPayload))` -> `Signature { raw_signature, signed_transaction? }` | wallet resolves `/product/<dotNs>/<idx>`, signs sr25519                                                                                                                                                                                                                        |
| `sign_raw`(114)                                                     | `signingRequest(rawPayload(SigningRawPayload))`                                                             | `.bytes` or `.payload` (string wrapped `<Bytes>…</Bytes>`)                                                                                                                                                                                                                     |
| `sign_payload_with_legacy_account` / `sign_raw_with_legacy_account` | same signing messages with product-derived account tuple                                                    | current dotli validates the legacy signer/address against `(calling_product_id, 0)` before forwarding                                                                                                                                                                          |
| `create_transaction`(30)                                            | `createTransactionRequest`                                                                                  | wallet assembles + signs the extrinsic                                                                                                                                                                                                                                         |
| `create_transaction_with_legacy_account`                            | same create-transaction message with product-derived signer tuple                                           | current dotli implements this and should remain parity                                                                                                                                                                                                                         |
| `get_account_alias`(24)                                             | `aliasRequest { product_account_id, calling_product_id }` -> `ContextualAlias { context:[u8;32], alias }`   | bandersnatch `deriveAlias`, auto-handled (no prompt)                                                                                                                                                                                                                           |
| `create_account_proof`(26)                                          | (full ring-VRF proof)                                                                                       | current dotli has no full-proof message; the SSO channel exposes `deriveAlias` only today                                                                                                                                                                                      |
| `create_proof`(60) / `_authorized`(132)                             | none (in-core)                                                                                              | core signs the product `Statement` with its session sr25519 key (`ssSecret`); signer = the allowance-granted session account                                                                                                                                                   |
| `subscribe`(56) / `submit`(62)                                      | none (in-core)                                                                                              | product statements over the same People-chain store the core is already on                                                                                                                                                                                                     |
| `request_resource_allocation`                                       | `resourceAllocationRequest` -> response                                                                     | current dotli prompts, sends `callingProductId`, maps allowance dialects, and strips secret material from allocated results                                                                                                                                                    |
| public logout/disconnect                                            | `disconnected`                                                                                              | best-effort send SSO peer message over the statement-store session channel, then tear down local session regardless of send result; peer-originated `disconnected` clears memory + `SessionStore`; any pending request/response waiters fail instead of waiting for a response |

### Full sequence

```
 Web product / Core                 People-chain statement store              Wallet
   request_login()
   mint session sr25519 kp + P256 kp
   build VersionedHandshakeProposal::V2; A1.present(QR) ------ scan ------------->| decode; show metadata
   subscribe(topic = H(ss_pub, enc_pub++"topic"))                                 | approve
                            <--- submit(topic, channel, VersionedHandshakeResponse::V2{enc, ephPub}) ---|
   verify proof; ECDH(enc_priv, ephPub)->decrypt; skip Pending until Success
     -> wallet persistent P256 pub, root_acct, identity_acct, root_entropy_source
   shared_secret = ECDH(enc_priv, sso_enc_pub_key)
   session_id.own/peer = H(shared_secret, "session"++..)
   SessionState{ public_key=root_acct, ss_secret, root_entropy_source, ... }; persist; broadcast Connected
   request_login -> Success
 ---- steady state (signing) ----
   sign_payload: submit(topic1=own, request_channel, enc(signingRequest)) ------>| sign /product/..
                            <--- submit(topic1=wallet_own, response_channel, enc(signingResponse)) ---|
   disconnect: best-effort submit(enc(disconnected)); stop subscriptions; fail pending waits; clear store
```

## What `SessionState` must hold (-> [C](<C - session-contract.md>))

session sr25519 statement keypair (pub + secret = `ssSecret`); P-256 ECDH keypair; the wallet's
`root_user_account_id` (= `public_key`, the product-account derivation root) and `identity_account_id`;
the persistent peer P-256 pubkey; derived `session_id.own/peer`; and the SSO V2 `rootEntropySource` used
for product entropy derivation. Usernames are **not** in the handshake; they come from a separate on-chain
People-chain identity lookup keyed by `root_user_account_id`.

## Crypto inventory (-> [D](<D - crypto-foundation.md>))

sr25519 (schnorrkel): statement proof sign + peer-proof verify, product-account HDKD. **P-256
(secp256r1) ECDH + HKDF-SHA256 + AES-GCM**: the channel. **Keyed BLAKE2b-256**: handshake topic,
session ids, and request/response channels. `useragent-kit` is useful implementation precedent for
similar Rust migrations, not a protocol authority for this byte contract. All must build for `wasm32`.

## Vector-gated items

These are no longer design questions for current dotli parity, but they still need golden vectors before
implementation to catch byte-level mistakes:

1. QR handshake SCALE/deeplink bytes, including optional host metadata fields.
2. P-256 key generation from entropy-derived seed, ECDH X-coordinate extraction, HKDF-SHA256
   empty-salt/info AES-GCM key derivation, and nonce+ciphertext framing.
3. Keyed BLAKE2b-256 handshake topic, session id, and request/response channel derivation.
4. Statement proof signing bytes: unsigned statement SCALE with compact length prefix stripped, 64-byte
   expanded `ssSecret`, and `sr25519_sign(publicKey, secret, message)`.
5. `create_account_proof`(26) remains deferred: current `RemoteMessageCodec` has alias request/response
   messages only, not a full ring-VRF proof message.
