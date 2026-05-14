import type { ObservableLike, Result, TrUApiClient } from "@parity/truapi";
import {
  awaitChainHeadOperation,
  awaitChainHeadStorage,
  getClient,
  openEphemeralFollow,
  type EphemeralFollow,
} from "./transport";

export type CallResult = { ok: boolean; data: unknown };

export type SubscriptionHandle = {
  unsubscribe: () => void;
  subscriptionId: string;
};

export type MethodBinding =
  | { isStream: false; call: (req: unknown) => Promise<CallResult> }
  | {
      isStream: true;
      subscribe: (
        req: unknown,
        onEvent: (data: unknown) => void,
        onEnd: (error?: Error) => void,
      ) => SubscriptionHandle;
    };

// Maps `${ServiceName}/${MethodName}` (UI label) ->
// [serviceField on TrUApiClient, methodName on the service class, isStream].
//
// The generated client encodes requests with the selected versioned envelope.
// Callers pass the inner request value directly; subscriptions return an
// Observable-like object whose `subscribe` call returns the transport-assigned
// subscription id.
const methodMap: Record<string, [keyof TrUApiClient, string, boolean]> = {
  "Account/connection_status_subscribe": [
    "account",
    "connectionStatusSubscribe",
    true,
  ],
  "Account/get_account": ["account", "getAccount", false],
  "Account/get_account_alias": ["account", "getAccountAlias", false],
  "Account/create_account_proof": ["account", "createAccountProof", false],
  "Account/get_legacy_accounts": ["account", "getLegacyAccounts", false],
  "Account/get_user_id": ["account", "getUserId", false],
  "Account/request_login": ["account", "requestLogin", false],

  "Chain/follow_head_subscribe": ["chain", "followHeadSubscribe", true],
  "Chain/get_head_header": ["chain", "getHeadHeader", false],
  "Chain/get_head_body": ["chain", "getHeadBody", false],
  "Chain/get_head_storage": ["chain", "getHeadStorage", false],
  "Chain/call_head": ["chain", "callHead", false],
  "Chain/unpin_head": ["chain", "unpinHead", false],
  "Chain/continue_head": ["chain", "continueHead", false],
  "Chain/stop_head_operation": ["chain", "stopHeadOperation", false],
  "Chain/get_spec_genesis_hash": ["chain", "getSpecGenesisHash", false],
  "Chain/get_spec_chain_name": ["chain", "getSpecChainName", false],
  "Chain/get_spec_properties": ["chain", "getSpecProperties", false],
  "Chain/broadcast_transaction": ["chain", "broadcastTransaction", false],
  "Chain/stop_transaction": ["chain", "stopTransaction", false],

  "Chat/create_room": ["chat", "createRoom", false],
  "Chat/register_bot": ["chat", "registerBot", false],
  "Chat/list_subscribe": ["chat", "listSubscribe", true],
  "Chat/post_message": ["chat", "postMessage", false],
  "Chat/action_subscribe": ["chat", "actionSubscribe", true],
  "Chat/custom_message_render_subscribe": [
    "chat",
    "customMessageRenderSubscribe",
    true,
  ],

  "Entropy/derive": ["entropy", "derive", false],

  "JSON-RPC/send_message": ["jsonRpc", "sendMessage", false],
  "JSON-RPC/subscribe_messages": ["jsonRpc", "subscribeMessages", true],

  "Local Storage/read": ["localStorage", "read", false],
  "Local Storage/write": ["localStorage", "write", false],
  "Local Storage/clear": ["localStorage", "clear", false],

  "Payment/balance_subscribe": ["payment", "balanceSubscribe", true],
  "Payment/request": ["payment", "request", false],
  "Payment/status_subscribe": ["payment", "statusSubscribe", true],
  "Payment/top_up": ["payment", "topUp", false],

  "Permissions/request_device_permission": [
    "permissions",
    "requestDevicePermission",
    false,
  ],
  "Permissions/request_remote_permission": [
    "permissions",
    "requestRemotePermission",
    false,
  ],

  "Preimage/lookup_subscribe": ["preimage", "lookupSubscribe", true],
  "Preimage/submit": ["preimage", "submit", false],

  "Resource Allocation/request": ["resourceAllocation", "request", false],

  "Signing/sign_raw_with_legacy_account": [
    "signing",
    "signRawWithLegacyAccount",
    false,
  ],
  "Signing/sign_payload_with_legacy_account": [
    "signing",
    "signPayloadWithLegacyAccount",
    false,
  ],
  "Signing/sign_raw": ["signing", "signRaw", false],
  "Signing/sign_payload": ["signing", "signPayload", false],

  "Statement Store/subscribe": ["statementStore", "subscribe", true],
  "Statement Store/create_proof": ["statementStore", "createProof", false],
  "Statement Store/create_proof_authorized": [
    "statementStore",
    "createProofAuthorized",
    false,
  ],
  "Statement Store/submit": ["statementStore", "submit", false],

  "System/handshake": ["system", "handshake", false],
  "System/feature_supported": ["system", "featureSupported", false],
  "System/push_notification": ["system", "pushNotification", false],
  "System/navigate_to": ["system", "navigateTo", false],

  "Theme/subscribe": ["theme", "subscribe", true],

  "Signing/create_transaction": ["signing", "createTransaction", false],
  "Signing/create_transaction_with_legacy_account": [
    "signing",
    "createTransactionWithLegacyAccount",
    false,
  ],
};

// Dependent chain-head methods that require an active follow subscription.
// When the caller leaves `followSubscriptionId` empty, the binding opens an
// ephemeral one for the request's `genesisHash`, fills the subscription id in
// (and a finalized block hash if the caller used the zero sentinel), and
// unsubscribes once the dependent call (and any matching operation events)
// settle.
const CHAIN_HEAD_DEPENDENT = new Set<string>([
  "Chain/get_head_header",
  "Chain/get_head_body",
  "Chain/get_head_storage",
  "Chain/call_head",
  "Chain/unpin_head",
  "Chain/continue_head",
  "Chain/stop_head_operation",
]);

const ZERO_HASH =
  "0x0000000000000000000000000000000000000000000000000000000000000000";

// Build the argument tuple for `fn.apply` against the generated client.
//
// The generated TS client uses two shapes:
//   - Unary: `methodName(request: T)` — a single positional arg, where `T`
//     is either an inner enum/value type or a struct-shaped versioned
//     wrapper. The bridge passes the user-typed JSON straight through.
//   - Subscribe: `methodName({ request: T })` — an options-object whose
//     `request` field carries the inner type. No-input subscribes drop the
//     argument entirely.
function buildArgs(req: unknown): unknown[] {
  const noParams = req === null || req === undefined;
  return noParams ? [] : [req];
}

function buildSubscribeArgs(req: unknown): unknown[] {
  const noParams = req === null || req === undefined;
  return noParams ? [] : [{ request: req }];
}

// Methods whose unary response is `Started { operationId }`, with the real
// result delivered later as event(s) on the parent follow stream. Each
// awaiter listens for events matching the operationId and returns the fields
// to merge into the response (alongside the original `start` status).
type OperationAwaiterCtx = {
  follow: EphemeralFollow;
  operationId: string;
};

const OPERATION_AWAITERS: Record<
  string,
  (ctx: OperationAwaiterCtx) => Promise<Record<string, unknown>>
> = {
  "Chain/get_head_body": async ({ follow, operationId }) => {
    const result = await awaitChainHeadOperation(follow, operationId, [
      "OperationBodyDone",
      "OperationError",
      "OperationInaccessible",
    ]);
    return { result };
  },
  "Chain/call_head": async ({ follow, operationId }) => {
    const result = await awaitChainHeadOperation(follow, operationId, [
      "OperationCallDone",
      "OperationError",
      "OperationInaccessible",
    ]);
    return { result };
  },
  "Chain/get_head_storage": async ({ follow, operationId }) => {
    const client = getClient();
    const result = await awaitChainHeadStorage(follow, operationId, {
      onWaitingForContinue: () => {
        Promise.resolve(
          client.chain.continueHead({
            genesisHash: follow.genesisHash,
            followSubscriptionId: follow.subscriptionId,
            operationId,
          }),
        ).catch(() => {
          /* benign: the await will time out and surface the error */
        });
      },
    });
    return { items: result.items, result: result.done };
  },
};

export function isMethodSupported(service: string, method: string): boolean {
  return `${service}/${method}` in methodMap;
}

type ClientMethod = (...args: unknown[]) => unknown;

function resolveClientMethod(
  client: TrUApiClient,
  service: keyof TrUApiClient,
  methodName: string,
): { fn: ClientMethod; thisArg: object } | null {
  const serviceObj = client[service] as unknown as Record<string, unknown>;
  if (!serviceObj) return null;
  const fn = serviceObj[methodName];
  if (typeof fn !== "function") return null;
  return { fn: fn as ClientMethod, thisArg: serviceObj };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function getMethodBinding(
  service: string,
  method: string,
): MethodBinding | null {
  const entry = methodMap[`${service}/${method}`];
  if (!entry) return null;

  const [serviceField, methodName, isStream] = entry;
  let client: TrUApiClient;
  try {
    client = getClient();
  } catch {
    // Not running inside a TrUAPI host: surface as "Not supported" so the
    // method browser stays usable for inspection.
    return null;
  }
  const resolved = resolveClientMethod(client, serviceField, methodName);
  if (!resolved) return null;
  const { fn, thisArg } = resolved;

  if (isStream) {
    return {
      isStream: true,
      subscribe(req, onEvent, onEnd) {
        const args = buildSubscribeArgs(normalizeForScale(req));
        const observable = fn.apply(thisArg, args) as ObservableLike<unknown>;
        const sub = observable.subscribe({
          next: onEvent,
          error: onEnd,
          complete: () => onEnd(),
        });
        return {
          subscriptionId: sub.subscriptionId,
          unsubscribe: () => {
            try {
              sub.unsubscribe();
            } catch {
              /* benign */
            }
          },
        };
      },
    };
  }

  const key = `${service}/${method}`;
  const needsEphemeralFollow = CHAIN_HEAD_DEPENDENT.has(key);
  const operationAwaiter = OPERATION_AWAITERS[key];

  return {
    isStream: false,
    async call(req) {
      let enriched = req;
      let ephemeralFollow: EphemeralFollow | null = null;
      let activeSubId = "";

      if (isPlainObject(req) && typeof req.followSubscriptionId === "string") {
        activeSubId = req.followSubscriptionId;
      }

      if (needsEphemeralFollow && isPlainObject(req)) {
        const hasSubId = activeSubId.length > 0;
        const genesisHash =
          typeof req.genesisHash === "string" ? req.genesisHash : "";
        if (!hasSubId && genesisHash) {
          ephemeralFollow = await openEphemeralFollow(
            genesisHash as `0x${string}`,
          );
          activeSubId = ephemeralFollow.subscriptionId;
          const next: Record<string, unknown> = {
            ...req,
            followSubscriptionId: ephemeralFollow.subscriptionId,
          };
          if (
            typeof req.hash === "string" &&
            (req.hash === "" || req.hash === ZERO_HASH)
          ) {
            next.hash = ephemeralFollow.finalizedBlockHash;
          }
          if (Array.isArray(req.hashes)) {
            next.hashes = req.hashes.map((h) =>
              typeof h === "string" && (h === "" || h === ZERO_HASH)
                ? ephemeralFollow!.finalizedBlockHash
                : h,
            );
          }
          enriched = next;
        }
      }

      try {
        const args = buildArgs(normalizeForScale(enriched));
        const result = (await fn.apply(thisArg, args)) as Result<
          unknown,
          unknown
        >;
        const matched: CallResult = result.match(
          (value) => ({ ok: true, data: value }),
          (error) => ({ ok: false, data: error }),
        );

        // body/storage/call return `Started { operationId }` synchronously, but
        // the actual result arrives async on the follow stream. Wait for the
        // matching event(s) so the playground can show them alongside the
        // original Started status.
        if (matched.ok && operationAwaiter && ephemeralFollow && activeSubId) {
          const status = matched.data as { tag?: string; value?: unknown };
          if (status?.tag === "Started") {
            const operationId = (
              status.value as { operationId?: string } | undefined
            )?.operationId;
            if (operationId) {
              const extra = await operationAwaiter({
                follow: ephemeralFollow,
                operationId,
              });
              return { ok: true, data: { start: matched.data, ...extra } };
            }
          }
        }

        return matched;
      } catch (error) {
        return {
          ok: false,
          data: {
            message: error instanceof Error ? error.message : String(error),
          },
        };
      } finally {
        ephemeralFollow?.unsubscribe();
      }
    },
  };
}

// Recursively prepares a JSON-parsed value for the generated client codecs:
//   - null              -> undefined         (JSON has no undefined; SCALE optionals need it)
//   - "123n"            -> BigInt(123)       (JSON has no BigInt; use the JS literal suffix convention)
// Hex-encoded byte fields stay as `0x...` strings: the generated codecs use
// `S.Hex()` (HexString) for `Vec<u8>` and `[u8; N]`, so no conversion needed.
function normalizeForScale(value: unknown): unknown {
  if (value === null) return undefined;
  if (typeof value === "string" && /^-?\d+n$/.test(value)) {
    return BigInt(value.slice(0, -1));
  }
  if (Array.isArray(value)) return value.map(normalizeForScale);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([k, v]) => [
        k,
        normalizeForScale(v),
      ]),
    );
  }
  return value;
}

// Stringify helper retained from the previous bridge so the response panel
// matches the request panel. Bigint is suffixed with `n` to round-trip with
// the request panel's parsing.
export function stringify(value: unknown): string {
  return JSON.stringify(
    value,
    (_, v) => (typeof v === "bigint" ? v.toString() + "n" : v),
    2,
  );
}
