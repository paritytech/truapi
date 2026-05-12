import assert from "node:assert/strict";

import { createTransport } from "../src/client.ts";
import { indexedTaggedUnion, Result, str, _void } from "../src/scale.ts";
import {
  createClient,
  SubscriptionError,
} from "../src/generated/client.ts";
import * as T from "../src/generated/types.ts";
import * as W from "../src/generated/wire-table.ts";
import { encodeWireMessage } from "../src/transport.ts";

/** Convert bytes to a lower-case hex string for readable assertions. */
function toHex(u) {
  return Array.from(u)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/** Return the successful result value or fail the assertion with context. */
function unwrap(result, message) {
  return result.match(
    (value) => value,
    (error) => assert.fail(`${message}: ${error.message}`),
  );
}

/** Create an in-memory provider plus helpers for injecting frames and closes. */
function providerFixture() {
  const sent = [];
  let listener = () => {};
  let closeListener = () => {};
  return {
    sent,
    provider: {
      postMessage(message) {
        sent.push(message);
      },
      subscribe(callback) {
        listener = callback;
        return () => {};
      },
      subscribeClose(callback) {
        closeListener = callback;
        return () => {};
      },
      dispose() {},
    },
    receive(message) {
      listener(message);
    },
    close(error) {
      closeListener(error);
    },
  };
}

/** Encode a V1 host-handshake response result payload. */
function handshakeResponsePayload(value) {
  return indexedTaggedUnion({
    V1: [0, Result(_void, T.HostHandshakeError)],
  }).enc({ tag: "V1", value });
}

// Generated methods pass inner values and encode the selected wire wrapper
// before handing bytes to the transport.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);

  const request = {
    productAccountId: {
      dotNsIdentifier: "foo",
      derivationIndex: 0,
    },
  };

  void client.accountManagement.accountGet(request);

  const expectedPayload = T.VersionedHostAccountGetRequest.enc({
    tag: "V1",
    value: request,
  });
  const expectedFrame = new Uint8Array(
    str.enc("p:1").length + 1 + expectedPayload.length,
  );
  expectedFrame.set(str.enc("p:1"), 0);
  expectedFrame[str.enc("p:1").length] = 22;
  expectedFrame.set(expectedPayload, str.enc("p:1").length + 1);

  assert.equal(toHex(fixture.sent[0]), toHex(expectedFrame));
  assert.equal(transport.truapiVersion, 1);
  assert.equal(transport.codecVersion, 1);
}

// Generated handshake calls use the transport's generated codec version; the
// caller does not pass `1` manually.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);

  void client.trUApiCalls.handshake();

  const expectedPayload = T.VersionedHostHandshakeRequest.enc({
    tag: "V1",
    value: { codecVersion: 1 },
  });
  const expectedFrame = new Uint8Array(
    str.enc("p:1").length + 1 + expectedPayload.length,
  );
  expectedFrame.set(str.enc("p:1"), 0);
  expectedFrame[str.enc("p:1").length] = 0;
  expectedFrame.set(expectedPayload, str.enc("p:1").length + 1);

  assert.equal(toHex(fixture.sent[0]), toHex(expectedFrame));
}

// Request responses are a versioned envelope around the result payload.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);

  const response = client.trUApiCalls.handshake();
  const frame = unwrap(
    encodeWireMessage({
      requestId: "p:1",
      payload: {
        id: W.HOST_HANDSHAKE.response,
        value: handshakeResponsePayload({ success: true, value: undefined }),
      },
    }),
    "encode handshake_response",
  );
  fixture.receive(frame);

  const result = await response;
  assert.equal(result.isOk(), true);
}

// Inbound handshake auto-response uses the same versioned-result response shape.
{
  const fixture = providerFixture();
  createTransport(fixture.provider);

  const requestPayload = T.VersionedHostHandshakeRequest.enc({
    tag: "V1",
    value: { codecVersion: 1 },
  });
  const requestFrame = unwrap(
    encodeWireMessage({
      requestId: "h:1",
      payload: {
        id: W.HOST_HANDSHAKE.request,
        value: requestPayload,
      },
    }),
    "encode inbound handshake_request",
  );
  fixture.receive(requestFrame);

  const expectedResponsePayload = handshakeResponsePayload({
    success: true,
    value: undefined,
  });
  const expectedFrame = unwrap(
    encodeWireMessage({
      requestId: "h:1",
      payload: {
        id: W.HOST_HANDSHAKE.response,
        value: expectedResponsePayload,
      },
    }),
    "encode expected handshake_response",
  );

  assert.equal(toHex(fixture.sent[0]), toHex(expectedFrame));
}

// Receive frames are decoded as wire wrappers by the generated observable, then
// delivered as inner values.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  const events = [];

  const sub = client.accountManagement
    .accountConnectionStatusSubscribe()
    .subscribe({
      next: (value) => events.push(value),
    });

  const frame = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        id: W.HOST_ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.receive,
        value: T.VersionedHostAccountConnectionStatusSubscribeItem.enc({
          tag: "V1",
          value: { tag: "Connected", value: undefined },
        }),
      },
    }),
    "encode receive",
  );
  fixture.receive(frame);

  assert.deepEqual(events, [{ tag: "Connected", value: undefined }]);
}

// Interrupt frames are payloadless terminators and complete the observable.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  const completions = [];

  const sub = client.accountManagement
    .accountConnectionStatusSubscribe()
    .subscribe({
      complete: (...args) => completions.push(args),
    });

  const frame = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        id: W.HOST_ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.interrupt,
        value: _void.enc(undefined),
      },
    }),
    "encode interrupt",
  );
  fixture.receive(frame);

  assert.deepEqual(completions, [[]]);
}

// Payment subscriptions carry typed interrupt payloads. Those are observable
// errors, not normal completion.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  const completions = [];
  const errors = [];

  const sub = client.payment.paymentBalanceSubscribe().subscribe({
    complete: () => completions.push(true),
    error: (error) => errors.push(error),
  });

  const reason = { tag: "PermissionDenied", value: undefined };
  const frame = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        id: W.HOST_PAYMENT_BALANCE_SUBSCRIBE.interrupt,
        value: T.VersionedHostPaymentBalanceSubscribeError.enc({
          tag: "V1",
          value: reason,
        }),
      },
    }),
    "encode typed payment interrupt",
  );
  fixture.receive(frame);

  assert.deepEqual(completions, []);
  assert.equal(errors.length, 1);
  assert.ok(errors[0] instanceof SubscriptionError);
  assert.deepEqual(errors[0].reason, reason);
  assert.equal(fixture.sent.length, 1);
}

// Malformed receive payloads are terminal observable errors. The generated
// wrapper sends `_stop` and ignores later receive frames for that subscription.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  const events = [];
  const errors = [];

  const sub = client.accountManagement
    .accountConnectionStatusSubscribe()
    .subscribe({
      next: (value) => events.push(value),
      error: (error) => errors.push(error),
    });

  const malformedFrame = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        id: W.HOST_ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.receive,
        value: _void.enc(undefined),
      },
    }),
    "encode malformed receive",
  );
  fixture.receive(malformedFrame);

  assert.deepEqual(events, []);
  assert.equal(errors.length, 1);
  assert.ok(errors[0] instanceof SubscriptionError);
  assert.equal(errors[0].reason, undefined);
  assert.equal(fixture.sent.length, 2);
  const expectedStop = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        id: W.HOST_ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.stop,
        value: _void.enc(undefined),
      },
    }),
    "encode stop after malformed receive",
  );
  assert.equal(toHex(fixture.sent[1]), toHex(expectedStop));

  const validFrame = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        id: W.HOST_ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.receive,
        value: T.VersionedHostAccountConnectionStatusSubscribeItem.enc({
          tag: "V1",
          value: { tag: "Connected", value: undefined },
        }),
      },
    }),
    "encode receive after malformed receive",
  );
  fixture.receive(validFrame);

  assert.deepEqual(events, []);
}

// Unsubscribe sends the protocol `_stop` frame and does not call terminal
// observer callbacks locally.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  const completions = [];
  const errors = [];

  const sub = client.accountManagement
    .accountConnectionStatusSubscribe()
    .subscribe({
      complete: () => completions.push(true),
      error: (error) => errors.push(error),
    });
  sub.unsubscribe();

  const expectedStop = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        id: W.HOST_ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.stop,
        value: _void.enc(undefined),
      },
    }),
    "encode explicit unsubscribe stop",
  );
  assert.equal(toHex(fixture.sent[1]), toHex(expectedStop));
  assert.deepEqual(completions, []);
  assert.deepEqual(errors, []);
}

// Provider close/error is a terminal observable error.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  const errors = [];

  client.accountManagement.accountConnectionStatusSubscribe().subscribe({
    error: (error) => errors.push(error),
  });

  const providerError = new Error("provider closed");
  fixture.close(providerError);

  assert.equal(errors.length, 1);
  assert.ok(errors[0] instanceof SubscriptionError);
  assert.equal(errors[0].message, "provider closed");
  assert.equal(errors[0].reason, undefined);
  assert.equal(errors[0].cause, providerError);
}

console.log("transport version wrapping tests passed");
