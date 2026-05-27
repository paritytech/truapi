---
title: "TrUAPI Protocol Design"
type: design
status: accepted
created: 2026-03-13
---

# TrUAPI Protocol Design

## Overview

The TrUAPI protocol connects a **Product** — a web application — with its **Host**, the native Polkadot application that embeds it. The two run in separate execution contexts (an iframe or webview inside a native shell) and share no memory; everything they exchange has to cross a process boundary as raw bytes.

This document specifies the **transport layer**: the rules for turning a method call on one side into bytes on the wire and back into a typed result on the other. It deliberately stops there. The concrete call surface — the methods themselves, their request/response types, error enums, and the wire-protocol discriminant ids — is defined in the `truapi` crate (`rust/crates/truapi`) and the clients generated from it. Keeping the two apart lets the API surface grow without disturbing the transport rules underneath it.

TrUAPI is language-agnostic. The examples below are written in Rust, but the protocol assumes nothing Rust-specific; any language that can serialize the same byte layout can speak it.

## Technical Requirements

A Host and a Product may be built on different platforms — web, iOS, Android — and in different languages. The transport therefore makes no assumptions about either side beyond a shared byte channel:

- The protocol MUST provide a transport layer between Host and Product over an arbitrary byte channel.
- The message format MUST be well-defined and serializable, so that an encoder on one platform and a decoder on another agree byte-for-byte.

## Transport

Communication between Host and Product can be carried over any IPC mechanism — a `MessagePort`, `postMessage` across an iframe boundary, or anything else that moves bytes. The transport treats that channel as opaque: the body of each IPC message is a single serialized `Message` (a byte array), and how those bytes actually travel is left to the environment.

Because the channel carries nothing but bytes, both sides must agree precisely on how a `Message` is laid out. That agreement is the serialization format.

### Serialization

Messages are plain structs and enums that are serialized into bytes on one side and decoded back into the same shape on the other.

Message serialization is built on [JAM codec](https://github.com/paritytech/jam-codec). The codec is positional — it writes no field names or tags, only values in declaration order — so **the field order of structs and the variant order of enums are part of the wire contract**; reordering them silently breaks compatibility. The examples in this document omit the codec derive calls, but they are always implied. `Result` is treated as an ordinary serializable enum.

#### Note on JAM codec

[JAM codec](https://github.com/paritytech/jam-codec) is based on SCALE codec, adding native support for the `Compact` type — a variable-length integer encoding that keeps small numbers small on the wire.

### Interface

Every message on the wire shares one envelope:

```rust
struct Message {
  requestId: str,
  payload: Payload
}
```

`requestId` ties related messages together (see [Rules](#rules)); `payload` carries the action itself. `Payload` is an enum whose variants are the **actions** — the individual things a Host and Product can say to each other.

Actions are not written by hand. They are derived mechanically from the TrUAPI methods, so the high-level method signature and the wire format can never drift apart. One method expands into several actions depending on its shape: a plain call becomes a request/response pair, while a subscription becomes a small lifecycle of start, stop, interrupt, and receive messages.

Each action variant carries an explicit wire-protocol discriminant — its `request_id`, `response_id`, `start_id`, `stop_id`, `interrupt_id`, or `receive_id`. These ids are assigned per method in the `truapi` crate via the `#[wire(...)]` annotation. They are **append-only and never reused**: once an id ships it keeps its meaning forever, which is what lets a newer Host and an older Product still understand each other. The crate is the source of truth for their values.

Payloads are versioned independently of the action id, so a single message can evolve without renumbering anything around it. The current version `V1` encodes as discriminant `0`:

```rust
enum Versioned<T> {
  V1(T),
  // ...
}
```

Actions are derived from the TrUAPI methods using the following algorithm:

- For request functions, actions are derived as follows:
  - Request
    - Name: `method_name + '_request'`
    - Argument: `Versioned<(arg1, arg2, ...)>`
    - Discriminant: `request_id`
  - Response
    - Name: `method_name + '_response'`
    - Argument: `Versioned<Result<ReturnValue, ReturnError>>`
    - Discriminant: `response_id`
- For subscriptions, there are four messages:
  - Subscribe
    - Name: `method_name + '_start'`
    - Argument: tuple of all arguments except the callback `Versioned<(arg1, arg2, ...)>`
    - Discriminant: `start_id`
  - Unsubscribe
    - Name: `method_name + '_stop'`
    - Argument: none
    - Discriminant: `stop_id`
  - Interrupt
    - Name: `method_name + '_interrupt'`
    - Argument: none
    - Discriminant: `interrupt_id`
  - Receive
    - Name: `method_name + '_receive'`
    - Argument: the versioned callback argument `Versioned<CallbackArg>`
    - Discriminant: `receive_id`

Put together, a slice of `Payload` looks like this (the payload types are illustrative; see the `truapi` crate for the real ones):

```rust
enum Payload {
  host_handshake_request(Versioned::V1(HandshakeVersion)),
  host_handshake_response(Versioned::V1(Result<(), GenericErr>)),

  // ...
  // imaginary subscription method

  message_send_request(Versioned::V1((ChainId, str))),
  message_send_response(Versioned::V1(Result<(), GenericErr>)),

  message_subscribe_start(Versioned::V1(ChainId)),
  message_subscribe_stop,
  message_subscribe_interrupt,
  message_subscribe_receive(Versioned::V1(str)),

  // ...
}
```

### Rules

A single byte channel carries every call in both directions at once, so the two sides need a way to tell which message belongs to which exchange. That is what `requestId` is for.

#### Requests

Every request expects exactly one response. Each Host or Product MUST send a response message for every request it receives, and the request and its response MUST share the same `requestId` — so the caller can match a reply to the call it made even with many calls in flight.

#### Subscription

A subscription is not a one-shot call but an ongoing stream: the consumer asks once and then receives updates until it stops listening. Its four messages — `start`, `stop`, `interrupt`, and `receive` — MUST all share the same `requestId`, so a subscription handler can route every update and teardown signal to the right place.

Each message has a defined role:

- `start` — the consumer subscribes; it MUST send a `start` message to the provider.
- `stop` — the consumer unsubscribes; it MUST send a `stop` message.
- `interrupt` — if the provider can no longer supply data, it CAN send an `interrupt` message; the consumer MAY react by notifying the application layer.
- `receive` — the provider MUST deliver each update with a `receive` message.

The returned `Subscriber` interface depends on the implementation, but a generic one may look like this:

```rust
struct Subscriber {
  unsubscribe: fn(),
  onInterrupt: fn(fn())
}
```

### Handshake

Before either side trusts a single byte of payload, they have to agree on how those bytes are encoded. That negotiation is the handshake, and it runs first.

Handshake calls are bidirectional: both Host and Product can send a handshake request, and both MUST respond to one. An implementation CAN apply a timeout of 10 seconds, after which the connection is marked failed and the call returns a timeout error. The handshake result can be cached.

The handshake request carries the protocol (codec) version as a `u8`. On receiving it, the peer switches its encoding/decoding mode to match; for JAM codec, the version is `1`. A successful handshake MUST be the first request TrUAPI processes — any other request sent before a successful handshake response MUST fail.

The concrete handshake request, response, and error types are defined in the `truapi` crate.
