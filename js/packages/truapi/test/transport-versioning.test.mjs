import assert from "node:assert/strict";

import { createTransport } from "../src/client.ts";
import { indexedTaggedUnion, result, str, unit } from "../src/scale.ts";
import { createClient } from "../src/generated/client.ts";
import * as T from "../src/generated/types.ts";
import { encodeWireMessage } from "../src/transport.ts";

function toHex(u) {
  return Array.from(u)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function unwrap(result, message) {
  if (result.isErr()) throw new Error(`${message}: ${result.error.message}`);
  return result.value;
}

function providerFixture() {
  const sent = [];
  let listener = () => {};
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
      dispose() {},
    },
    receive(message) {
      listener(message);
    },
  };
}

function handshakeResponsePayload(value) {
  return indexedTaggedUnion({
    V1: [0, result(unit, T.HandshakeError)],
  }).enc({ tag: "V1", value });
}

// Generated methods pass inner values and encode the selected wire wrapper
// before handing bytes to the transport.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);

  void client.accountManagement.accountGet(["foo", 0]);

  const expectedPayload = T.HostAccountGetRequest.enc({
    tag: "V1",
    value: ["foo", 0],
  });
  const expectedFrame = new Uint8Array(
    str.enc("p:1").length + 1 + expectedPayload.length,
  );
  expectedFrame.set(str.enc("p:1"), 0);
  expectedFrame[str.enc("p:1").length] = 22;
  expectedFrame.set(expectedPayload, str.enc("p:1").length + 1);

  assert.equal(toHex(fixture.sent[0]), toHex(expectedFrame));
  assert.equal(transport.truapiVersion, 2);
  assert.equal(transport.codecVersion, 1);
}

// Generated handshake calls use the transport's generated codec version; the
// caller does not pass `1` manually.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);

  void client.trUApiCalls.handshake();

  const expectedPayload = T.HostHandshakeRequest.enc({
    tag: "V1",
    value: 1,
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
        tag: "host_handshake_response",
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

  const requestPayload = T.HostHandshakeRequest.enc({
    tag: "V1",
    value: 1,
  });
  const requestFrame = unwrap(
    encodeWireMessage({
      requestId: "h:1",
      payload: {
        tag: "host_handshake_request",
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
        tag: "host_handshake_response",
        value: expectedResponsePayload,
      },
    }),
    "encode expected handshake_response",
  );

  assert.equal(toHex(fixture.sent[0]), toHex(expectedFrame));
}

// Receive frames are decoded as wire wrappers by the transport, then delivered
// to generated callbacks as inner values.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  const events = [];

  const sub = client.accountManagement.accountConnectionStatusSubscribe({
    onData: (value) => events.push(value),
  });

  const frame = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        tag: "host_account_connection_status_subscribe_receive",
        value: T.HostAccountConnectionStatusItem.enc({
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

// Interrupt frames are payloadless terminators. Generated callbacks receive no
// typed error payload.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  const interrupts = [];

  const sub = client.accountManagement.accountConnectionStatusSubscribe({
    onData: () => {},
    onInterrupt: (...args) => interrupts.push(args),
  });

  const frame = unwrap(
    encodeWireMessage({
      requestId: sub.subscriptionId,
      payload: {
        tag: "host_account_connection_status_subscribe_interrupt",
        value: unit.enc(undefined),
      },
    }),
    "encode interrupt",
  );
  fixture.receive(frame);

  assert.deepEqual(interrupts, [[]]);
}

console.log("transport version wrapping tests passed");
