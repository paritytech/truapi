import type { Result } from "neverthrow";
import { describe, expect, it } from "bun:test";

import { createTransport } from "./client.js";
import { CallError, indexedTaggedUnion, Result as ScaleResult, str, _void } from "./scale.js";
import type { Codec } from "./scale.js";
import { createClient, SubscriptionError } from "./generated/client.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";
import { encodeWireMessage } from "./transport.js";

/** Wrap a codec in the `{ V1: [0, codec] }` indexed-tagged-union envelope. */
const versionedV1 = <T>(codec: Codec<T>) => indexedTaggedUnion({ V1: [0, codec] });

function toHex(u: Uint8Array): string {
    return Array.from(u)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
}

/** Return the successful result value or fail the test with context. */
function unwrap<T>(result: Result<T, { message: string }>, message: string): T {
    return result.match(
        (value) => value,
        (error): never => {
            throw new Error(`${message}: ${error.message}`);
        },
    );
}

/** Create an in-memory provider plus helpers for injecting frames and closes. */
function providerFixture() {
    const sent: Uint8Array[] = [];
    let listener: (message: Uint8Array) => void = () => {};
    let closeListener: (error: Error) => void = () => {};
    return {
        sent,
        provider: {
            postMessage(message: Uint8Array) {
                sent.push(message);
            },
            subscribe(callback: (message: Uint8Array) => void) {
                listener = callback;
                return () => {};
            },
            subscribeClose(callback: (error: Error) => void) {
                closeListener = callback;
                return () => {};
            },
            dispose() {},
        },
        receive(message: Uint8Array) {
            listener(message);
        },
        close(error: Error) {
            closeListener(error);
        },
    };
}

/** Encode a V1 host-handshake response result payload. */
function handshakeResponsePayload(value: { success: true; value: undefined }): Uint8Array {
    return versionedV1(ScaleResult(_void, CallError(T.VersionedHostHandshakeError))).enc({
        tag: "V1",
        value,
    });
}

function accountGetResponsePayload(
    value:
        | {
              success: true;
              value: T.HostAccountGetResponse;
          }
        | {
              success: false;
              value: { tag: "Domain"; value: T.VersionedHostAccountGetError };
          },
): Uint8Array {
    return versionedV1(
        ScaleResult(T.HostAccountGetResponse, CallError(T.VersionedHostAccountGetError)),
    ).enc({ tag: "V1", value });
}

/** Encode a raw testing echo error response payload. */
function testingEchoErrorPayload(reason: string): Uint8Array {
    return ScaleResult(_void, CallError(T.V01TestingVersionProbeError)).enc({
        success: false,
        value: { tag: "HostFailure", value: { reason } },
    });
}

describe("generated client transport", () => {
    it("encodes unit-only enums as a single-byte SCALE discriminant", () => {
        // Unit-only enums expose a string union on the public API while
        // preserving the same single-byte SCALE discriminant encoding.
        expect(toHex(T.HostDevicePermissionRequest.enc("Camera"))).toBe("01");
        expect(T.HostDevicePermissionRequest.dec(new Uint8Array([1]))).toBe("Camera");
    });

    it("wraps generated method requests in the selected wire wrapper", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const request = {
            productAccountId: { dotNsIdentifier: "foo", derivationIndex: 0 },
        };
        void client.account.getAccount(request);

        const expectedPayload = T.VersionedHostAccountGetRequest.enc({ tag: "V1", value: request });
        const expectedFrame = new Uint8Array(str.enc("p:1").length + 1 + expectedPayload.length);
        expectedFrame.set(str.enc("p:1"), 0);
        expectedFrame[str.enc("p:1").length] = 22;
        expectedFrame.set(expectedPayload, str.enc("p:1").length + 1);

        expect(toHex(fixture.sent[0])).toBe(toHex(expectedFrame));
    });

    it("uses the latest generated request version for testing probes", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const request = {
            message: "hello from test",
            marker: 42,
        };
        void client.testing.versionProbe(request);

        const expectedPayload = T.VersionedTestingVersionProbeRequest.enc({
            tag: "V2",
            value: request,
        });
        const expectedFrame = new Uint8Array(str.enc("p:1").length + 1 + expectedPayload.length);
        expectedFrame.set(str.enc("p:1"), 0);
        expectedFrame[str.enc("p:1").length] = W.TESTING_VERSION_PROBE.request;
        expectedFrame.set(expectedPayload, str.enc("p:1").length + 1);

        expect(toHex(fixture.sent[0])).toBe(toHex(expectedFrame));
    });

    it("uses the transport codec version for generated handshake calls", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        void client.system.handshake();

        const expectedPayload = T.VersionedHostHandshakeRequest.enc({
            tag: "V1",
            value: { codecVersion: 1 },
        });
        const expectedFrame = new Uint8Array(str.enc("p:1").length + 1 + expectedPayload.length);
        expectedFrame.set(str.enc("p:1"), 0);
        expectedFrame[str.enc("p:1").length] = 0;
        expectedFrame.set(expectedPayload, str.enc("p:1").length + 1);

        expect(toHex(fixture.sent[0])).toBe(toHex(expectedFrame));
    });

    it("resolves a request from its versioned response envelope", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const response = client.system.handshake();
        const frame = unwrap(
            encodeWireMessage({
                requestId: "p:1",
                payload: {
                    id: W.SYSTEM_HANDSHAKE.response,
                    value: handshakeResponsePayload({ success: true, value: undefined }),
                },
            }),
            "encode handshake_response",
        );
        fixture.receive(frame);

        const result = await response;
        expect(result.isOk()).toBe(true);
    });

    it("decodes request domain errors from the versioned response envelope", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const response = client.account.getAccount({
            productAccountId: { dotNsIdentifier: "foo", derivationIndex: 0 },
        });
        const reason = { tag: "V1", value: { tag: "NotConnected", value: undefined } } as const;
        const frame = unwrap(
            encodeWireMessage({
                requestId: "p:1",
                payload: {
                    id: W.ACCOUNT_GET_ACCOUNT.response,
                    value: accountGetResponsePayload({
                        success: false,
                        value: { tag: "Domain", value: reason },
                    }),
                },
            }),
            "encode account_get error response",
        );
        fixture.receive(frame);

        const result = await response;
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr()).toEqual({ tag: "Domain", value: reason });
    });

    it("returns framework call errors as typed Err values", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const response = client.testing.echoError({
            error: { tag: "HostFailure", value: { reason: "forced by test" } },
        });
        const frame = unwrap(
            encodeWireMessage({
                requestId: "p:1",
                payload: {
                    id: W.TESTING_ECHO_ERROR.response,
                    value: testingEchoErrorPayload("forced by test"),
                },
            }),
            "encode testing framework error response",
        );
        fixture.receive(frame);

        const result = await response;
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr()).toEqual({
            tag: "HostFailure",
            value: { reason: "forced by test" },
        });
    });

    it("auto-responds to an inbound handshake with the versioned-result shape", () => {
        const fixture = providerFixture();
        createTransport(fixture.provider);

        const requestPayload = T.VersionedHostHandshakeRequest.enc({
            tag: "V1",
            value: { codecVersion: 1 },
        });
        const requestFrame = unwrap(
            encodeWireMessage({
                requestId: "h:1",
                payload: { id: W.SYSTEM_HANDSHAKE.request, value: requestPayload },
            }),
            "encode inbound handshake_request",
        );
        fixture.receive(requestFrame);

        const expectedFrame = unwrap(
            encodeWireMessage({
                requestId: "h:1",
                payload: {
                    id: W.SYSTEM_HANDSHAKE.response,
                    value: handshakeResponsePayload({ success: true, value: undefined }),
                },
            }),
            "encode expected handshake_response",
        );
        expect(toHex(fixture.sent[0])).toBe(toHex(expectedFrame));
    });

    it("decodes receive frames as wire wrappers and delivers inner values", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const events: unknown[] = [];

        const sub = client.account
            .connectionStatusSubscribe()
            .subscribe({ next: (value) => events.push(value) });

        const frame = unwrap(
            encodeWireMessage({
                requestId: sub.subscriptionId,
                payload: {
                    id: W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.receive,
                    value: T.VersionedHostAccountConnectionStatusSubscribeItem.enc({
                        tag: "V1",
                        value: "Connected",
                    }),
                },
            }),
            "encode receive",
        );
        fixture.receive(frame);

        expect(events).toEqual(["Connected"]);
    });

    it("completes the observable on a payloadless interrupt terminator", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const completions: unknown[][] = [];

        const sub = client.account
            .connectionStatusSubscribe()
            .subscribe({ complete: (...args) => completions.push(args) });

        const frame = unwrap(
            encodeWireMessage({
                requestId: sub.subscriptionId,
                payload: {
                    id: W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.interrupt,
                    value: _void.enc(undefined),
                },
            }),
            "encode interrupt",
        );
        fixture.receive(frame);

        expect(completions).toEqual([[]]);
    });

    it("surfaces a typed payment interrupt as an observable error", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const completions: boolean[] = [];
        const errors: Error[] = [];

        const sub = client.payment.balanceSubscribe({ request: {} }).subscribe({
            complete: () => completions.push(true),
            error: (error) => errors.push(error),
        });

        const reason = { tag: "PermissionDenied", value: undefined } as const;
        const callError = {
            tag: "Domain",
            value: { tag: "V1", value: reason },
        } as const;
        const frame = unwrap(
            encodeWireMessage({
                requestId: sub.subscriptionId,
                payload: {
                    id: W.PAYMENT_BALANCE_SUBSCRIBE.interrupt,
                    value: versionedV1(CallError(T.VersionedHostPaymentBalanceSubscribeError)).enc({
                        tag: "V1",
                        value: callError,
                    }),
                },
            }),
            "encode typed payment interrupt",
        );
        fixture.receive(frame);

        expect(completions).toEqual([]);
        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect((errors[0] as SubscriptionError).reason).toEqual(callError);
        expect(fixture.sent).toHaveLength(1);
    });

    it("uses the same typed-interrupt envelope for RFC0017 coin-payment streams", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const errors: Error[] = [];

        const sub = client.coinPayment
            .rebalancePurse({ request: { from: 1, to: 2, amount: 1000 } })
            .subscribe({ error: (error) => errors.push(error) });

        const reason = "Denied";
        const callError = {
            tag: "Domain",
            value: { tag: "V1", value: reason },
        } as const;
        const frame = unwrap(
            encodeWireMessage({
                requestId: sub.subscriptionId,
                payload: {
                    id: W.COIN_PAYMENT_REBALANCE_PURSE.interrupt,
                    value: versionedV1(
                        CallError(T.VersionedHostCoinPaymentRebalancePurseError),
                    ).enc({ tag: "V1", value: callError }),
                },
            }),
            "encode typed coin payment interrupt",
        );
        fixture.receive(frame);

        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect((errors[0] as SubscriptionError).reason).toEqual(callError);
    });

    it("treats a malformed receive payload as terminal and sends _stop", () => {
        // After the error, the generated wrapper sends `_stop` and ignores later
        // receive frames for that subscription.
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const events: unknown[] = [];
        const errors: Error[] = [];

        const sub = client.account.connectionStatusSubscribe().subscribe({
            next: (value) => events.push(value),
            error: (error) => errors.push(error),
        });

        const malformedFrame = unwrap(
            encodeWireMessage({
                requestId: sub.subscriptionId,
                payload: {
                    id: W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.receive,
                    value: _void.enc(undefined),
                },
            }),
            "encode malformed receive",
        );
        fixture.receive(malformedFrame);

        expect(events).toEqual([]);
        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect((errors[0] as SubscriptionError).reason).toBeUndefined();
        expect(fixture.sent).toHaveLength(2);

        const expectedStop = unwrap(
            encodeWireMessage({
                requestId: sub.subscriptionId,
                payload: {
                    id: W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.stop,
                    value: _void.enc(undefined),
                },
            }),
            "encode stop after malformed receive",
        );
        expect(toHex(fixture.sent[1])).toBe(toHex(expectedStop));

        const validFrame = unwrap(
            encodeWireMessage({
                requestId: sub.subscriptionId,
                payload: {
                    id: W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.receive,
                    value: T.VersionedHostAccountConnectionStatusSubscribeItem.enc({
                        tag: "V1",
                        value: "Connected",
                    }),
                },
            }),
            "encode receive after malformed receive",
        );
        fixture.receive(validFrame);

        expect(events).toEqual([]);
    });

    it("sends _stop on unsubscribe without invoking terminal callbacks locally", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const completions: boolean[] = [];
        const errors: Error[] = [];

        const sub = client.account.connectionStatusSubscribe().subscribe({
            complete: () => completions.push(true),
            error: (error) => errors.push(error),
        });
        sub.unsubscribe();

        const expectedStop = unwrap(
            encodeWireMessage({
                requestId: sub.subscriptionId,
                payload: {
                    id: W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.stop,
                    value: _void.enc(undefined),
                },
            }),
            "encode explicit unsubscribe stop",
        );
        expect(toHex(fixture.sent[1])).toBe(toHex(expectedStop));
        expect(completions).toEqual([]);
        expect(errors).toEqual([]);
    });

    it("propagates a provider close/error as a terminal observable error", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const errors: Error[] = [];

        client.account
            .connectionStatusSubscribe()
            .subscribe({ error: (error) => errors.push(error) });

        const providerError = new Error("provider closed");
        fixture.close(providerError);

        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect(errors[0].message).toBe("provider closed");
        expect((errors[0] as SubscriptionError).reason).toBeUndefined();
        expect(errors[0].cause).toBe(providerError);
    });
});
