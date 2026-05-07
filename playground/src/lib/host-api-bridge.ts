import type { Result, Subscription, TrUApiClient } from "@parity/truapi";
import {
  awaitChainHeadOperation,
  awaitChainHeadStorage,
  getClient,
  hexToBytes,
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
        onEnd: () => void,
      ) => SubscriptionHandle;
    };

// Maps `${ServiceName}/${MethodName}` (UI label) ->
// [serviceField on TrUApiClient, methodName on the service class, isStream].
//
// The generated client encodes requests with the selected versioned envelope.
// Callers pass the inner request value directly; subscriptions take a callback
// and return a `Subscription` object that exposes the transport-assigned
// subscription id.
const methodMap: Record<string, [keyof TrUApiClient, string, boolean]> = {
  // TrUAPI Calls
  "TrUAPI Calls/host_handshake": ["trUApiCalls", "handshake", false],
  "TrUAPI Calls/host_feature_supported": [
    "trUApiCalls",
    "featureSupported",
    false,
  ],
  "TrUAPI Calls/host_navigate_to": ["trUApiCalls", "navigateTo", false],
  "TrUAPI Calls/host_push_notification": [
    "trUApiCalls",
    "pushNotification",
    false,
  ],

  // Permissions
  "Permissions/host_device_permission": [
    "permissions",
    "devicePermission",
    false,
  ],
  "Permissions/remote_permission": ["permissions", "permission", false],

  // Local Storage
  "Local Storage/host_local_storage_read": [
    "localStorage",
    "localStorageRead",
    false,
  ],
  "Local Storage/host_local_storage_write": [
    "localStorage",
    "localStorageWrite",
    false,
  ],
  "Local Storage/host_local_storage_clear": [
    "localStorage",
    "localStorageClear",
    false,
  ],

  // Account Management
  "Account Management/host_account_get": [
    "accountManagement",
    "accountGet",
    false,
  ],
  "Account Management/host_account_get_alias": [
    "accountManagement",
    "accountGetAlias",
    false,
  ],
  "Account Management/host_account_create_proof": [
    "accountManagement",
    "accountCreateProof",
    false,
  ],
  "Account Management/host_get_legacy_accounts": [
    "accountManagement",
    "getLegacyAccounts",
    false,
  ],
  "Account Management/host_account_connection_status_subscribe": [
    "accountManagement",
    "accountConnectionStatusSubscribe",
    true,
  ],
  "Account Management/host_get_user_id": [
    "accountManagement",
    "getUserId",
    false,
  ],

  // Signing
  "Signing/host_sign_payload": ["signing", "signPayload", false],
  "Signing/host_sign_raw": ["signing", "signRaw", false],
  "Signing/host_create_transaction": ["signing", "createTransaction", false],
  "Signing/host_create_transaction_with_legacy_account": [
    "signing",
    "createTransactionWithLegacyAccount",
    false,
  ],

  // Chat
  "Chat/host_chat_create_room": ["chat", "chatCreateRoom", false],
  "Chat/host_chat_create_simple_group": [
    "chat",
    "chatCreateSimpleGroup",
    false,
  ],
  "Chat/host_chat_register_bot": ["chat", "chatRegisterBot", false],
  "Chat/host_chat_post_message": ["chat", "chatPostMessage", false],
  "Chat/host_chat_list_subscribe": ["chat", "chatListSubscribe", true],
  "Chat/host_chat_action_subscribe": ["chat", "chatActionSubscribe", true],
  "Chat/product_chat_custom_message_render_subscribe": [
    "chat",
    "chatCustomMessageRenderSubscribe",
    true,
  ],

  // Statement Store
  "Statement Store/remote_statement_store_subscribe": [
    "statementStore",
    "statementStoreSubscribe",
    true,
  ],
  "Statement Store/remote_statement_store_create_proof": [
    "statementStore",
    "statementStoreCreateProof",
    false,
  ],
  "Statement Store/remote_statement_store_submit": [
    "statementStore",
    "statementStoreSubmit",
    false,
  ],

  // Preimage
  "Preimage/remote_preimage_lookup_subscribe": [
    "preimage",
    "preimageLookupSubscribe",
    true,
  ],

  // Chain Interaction
  "Chain Interaction/remote_chain_head_follow": [
    "chainInteraction",
    "chainHeadFollow",
    true,
  ],
  "Chain Interaction/remote_chain_head_header": [
    "chainInteraction",
    "chainHeadHeader",
    false,
  ],
  "Chain Interaction/remote_chain_head_body": [
    "chainInteraction",
    "chainHeadBody",
    false,
  ],
  "Chain Interaction/remote_chain_head_storage": [
    "chainInteraction",
    "chainHeadStorage",
    false,
  ],
  "Chain Interaction/remote_chain_head_call": [
    "chainInteraction",
    "chainHeadCall",
    false,
  ],
  "Chain Interaction/remote_chain_head_unpin": [
    "chainInteraction",
    "chainHeadUnpin",
    false,
  ],
  "Chain Interaction/remote_chain_head_continue": [
    "chainInteraction",
    "chainHeadContinue",
    false,
  ],
  "Chain Interaction/remote_chain_head_stop_operation": [
    "chainInteraction",
    "chainHeadStopOperation",
    false,
  ],
  "Chain Interaction/remote_chain_spec_genesis_hash": [
    "chainInteraction",
    "chainSpecGenesisHash",
    false,
  ],
  "Chain Interaction/remote_chain_spec_chain_name": [
    "chainInteraction",
    "chainSpecChainName",
    false,
  ],
  "Chain Interaction/remote_chain_spec_properties": [
    "chainInteraction",
    "chainSpecProperties",
    false,
  ],
  "Chain Interaction/remote_chain_transaction_broadcast": [
    "chainInteraction",
    "chainTransactionBroadcast",
    false,
  ],
  "Chain Interaction/remote_chain_transaction_stop": [
    "chainInteraction",
    "chainTransactionStop",
    false,
  ],

  // Payment
  "Payment/host_payment_balance_subscribe": [
    "payment",
    "paymentBalanceSubscribe",
    true,
  ],
  "Payment/host_payment_top_up": ["payment", "paymentTopUp", false],
  "Payment/host_payment_request": ["payment", "paymentRequest", false],
  "Payment/host_payment_status_subscribe": [
    "payment",
    "paymentStatusSubscribe",
    true,
  ],

  // Entropy
  "Entropy Derivation/host_derive_entropy": [
    "entropyDerivation",
    "deriveEntropy",
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
  "Chain Interaction/remote_chain_head_header",
  "Chain Interaction/remote_chain_head_body",
  "Chain Interaction/remote_chain_head_storage",
  "Chain Interaction/remote_chain_head_call",
  "Chain Interaction/remote_chain_head_unpin",
  "Chain Interaction/remote_chain_head_continue",
  "Chain Interaction/remote_chain_head_stop_operation",
]);

const ZERO_HASH =
  "0x0000000000000000000000000000000000000000000000000000000000000000";

// Build the argument tuple for `fn.apply` against the generated client.
//
// The generated TS client uses two shapes:
//   - Unary: `methodName(request: T)` — a single positional arg, where `T`
//     is either an inner enum/value type or a struct-shaped versioned
//     wrapper. The bridge passes the user-typed JSON straight through.
//   - Subscribe: `methodName({ request?, onData, onInterrupt? })` — a
//     single options object combining the request value with the
//     callbacks. No-input subscribes drop the `request` field.
function buildArgs(req: unknown, onData?: (data: unknown) => void): unknown[] {
  const noParams = req === null || req === undefined;
  if (onData) {
    return [noParams ? { onData } : { request: req, onData }];
  }
  return noParams ? [] : [req];
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
  "Chain Interaction/remote_chain_head_body": async ({
    follow,
    operationId,
  }) => {
    const result = await awaitChainHeadOperation(follow, operationId, [
      "OperationBodyDone",
      "OperationError",
      "OperationInaccessible",
    ]);
    return { result };
  },
  "Chain Interaction/remote_chain_head_call": async ({
    follow,
    operationId,
  }) => {
    const result = await awaitChainHeadOperation(follow, operationId, [
      "OperationCallDone",
      "OperationError",
      "OperationInaccessible",
    ]);
    return { result };
  },
  "Chain Interaction/remote_chain_head_storage": async ({
    follow,
    operationId,
  }) => {
    const client = getClient();
    const result = await awaitChainHeadStorage(follow, operationId, {
      onWaitingForContinue: () => {
        Promise.resolve(
          client.chainInteraction.chainHeadContinue({
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
        const args = buildArgs(normalizeForScale(req), onEvent);
        const sub = fn.apply(thisArg, args) as Subscription;
        return {
          subscriptionId: sub.subscriptionId,
          unsubscribe: () => {
            try {
              sub.unsubscribe();
            } catch {
              /* benign */
            }
            onEnd();
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
//   - "0x.."            -> Uint8Array        (any 0x-prefixed even-length hex string is treated as bytes)
//   - { bytes: "0x.." } -> Uint8Array        (explicit envelope, kept for symmetry with stringify())
//   - Uint8Array        -> Uint8Array        (pass-through, do not recurse)
function normalizeForScale(value: unknown): unknown {
  if (value === null) return undefined;
  if (value instanceof Uint8Array) return value;
  if (typeof value === "string") {
    if (/^-?\d+n$/.test(value)) return BigInt(value.slice(0, -1));
    if (/^0x[0-9a-fA-F]*$/.test(value) && value.length % 2 === 0)
      return hexToBytes(value);
    return value;
  }
  if (Array.isArray(value)) return value.map(normalizeForScale);
  if (typeof value === "object") {
    const obj = value as Record<string, unknown>;
    if (
      Object.keys(obj).length === 1 &&
      typeof obj["bytes"] === "string" &&
      /^0x[0-9a-fA-F]*$/.test(obj["bytes"] as string)
    ) {
      return hexToBytes(obj["bytes"] as string);
    }
    return Object.fromEntries(
      Object.entries(obj).map(([k, v]) => [k, normalizeForScale(v)]),
    );
  }
  return value;
}

// Stringify helper retained from the previous bridge so the response panel
// matches the request panel. Uint8Array -> {bytes:"0x.."}, bigint -> "<n>n".
export function stringify(value: unknown): string {
  return JSON.stringify(
    value,
    (_, v) => {
      if (v instanceof Uint8Array) {
        return {
          bytes:
            "0x" +
            Array.from(v)
              .map((b) => b.toString(16).padStart(2, "0"))
              .join(""),
        };
      }
      if (typeof v === "bigint") return v.toString() + "n";
      return v;
    },
    2,
  );
}
