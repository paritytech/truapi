import type { TrUApiClient } from '@truapi/client';
import { getClient } from './transport';

export type CallResult = { ok: boolean; data: unknown };

export type MethodBinding =
  | { isStream: false; call: (req: unknown) => Promise<CallResult> }
  | {
      isStream: true;
      subscribe: (
        req: unknown,
        onEvent: (data: unknown) => void,
        onEnd: () => void,
      ) => { unsubscribe: () => void };
    };

// Maps `${ServiceName}/${MethodName}` (UI label) ->
// [serviceField on TrUApiClient, methodName on the service class, isStream].
//
// The new generated client wraps requests in V2 internally. Callers pass the
// inner request value directly; subscriptions take a callback and return
// Unsubscribe.
const methodMap: Record<
  string,
  [keyof TrUApiClient, string, boolean]
> = {
  // TrUAPI Calls
  'TrUAPI Calls/host_handshake': ['trUApiCalls', 'handshake', false],
  'TrUAPI Calls/host_feature_supported': ['trUApiCalls', 'featureSupported', false],
  'TrUAPI Calls/host_navigate_to': ['trUApiCalls', 'navigateTo', false],
  'TrUAPI Calls/host_push_notification': ['trUApiCalls', 'pushNotification', false],

  // Permissions
  'Permissions/host_device_permission': ['permissions', 'devicePermission', false],
  'Permissions/remote_permission': ['permissions', 'permission', false],

  // Local Storage
  'Local Storage/host_local_storage_read': ['localStorage', 'localStorageRead', false],
  'Local Storage/host_local_storage_write': ['localStorage', 'localStorageWrite', false],
  'Local Storage/host_local_storage_clear': ['localStorage', 'localStorageClear', false],

  // Account Management
  'Account Management/host_account_get': ['accountManagement', 'accountGet', false],
  'Account Management/host_account_get_alias': ['accountManagement', 'accountGetAlias', false],
  'Account Management/host_account_create_proof': ['accountManagement', 'accountCreateProof', false],
  'Account Management/host_get_non_product_accounts': ['accountManagement', 'getNonProductAccounts', false],
  'Account Management/host_account_connection_status_subscribe': ['accountManagement', 'accountConnectionStatusSubscribe', true],
  'Account Management/host_get_user_id': ['accountManagement', 'getUserId', false],

  // Signing
  'Signing/host_sign_payload': ['signing', 'signPayload', false],
  'Signing/host_sign_raw': ['signing', 'signRaw', false],
  'Signing/host_create_transaction': ['signing', 'createTransaction', false],
  'Signing/host_create_transaction_with_non_product_account': ['signing', 'createTransactionWithNonProductAccount', false],

  // Chat
  'Chat/host_chat_create_room': ['chat', 'chatCreateRoom', false],
  'Chat/host_chat_create_simple_group': ['chat', 'chatCreateSimpleGroup', false],
  'Chat/host_chat_register_bot': ['chat', 'chatRegisterBot', false],
  'Chat/host_chat_post_message': ['chat', 'chatPostMessage', false],
  'Chat/host_chat_list_subscribe': ['chat', 'chatListSubscribe', true],
  'Chat/host_chat_action_subscribe': ['chat', 'chatActionSubscribe', true],
  'Chat/product_chat_custom_message_render_subscribe': ['chat', 'chatCustomMessageRenderSubscribe', true],

  // Statement Store
  'Statement Store/remote_statement_store_subscribe': ['statementStore', 'statementStoreSubscribe', true],
  'Statement Store/remote_statement_store_create_proof': ['statementStore', 'statementStoreCreateProof', false],
  'Statement Store/remote_statement_store_submit': ['statementStore', 'statementStoreSubmit', false],

  // Preimage
  'Preimage/remote_preimage_lookup_subscribe': ['preimage', 'preimageLookupSubscribe', true],

  // Chain Interaction
  'Chain Interaction/remote_chain_head_follow': ['chainInteraction', 'chainHeadFollow', true],
  'Chain Interaction/remote_chain_head_header': ['chainInteraction', 'chainHeadHeader', false],
  'Chain Interaction/remote_chain_head_body': ['chainInteraction', 'chainHeadBody', false],
  'Chain Interaction/remote_chain_head_storage': ['chainInteraction', 'chainHeadStorage', false],
  'Chain Interaction/remote_chain_head_call': ['chainInteraction', 'chainHeadCall', false],
  'Chain Interaction/remote_chain_head_unpin': ['chainInteraction', 'chainHeadUnpin', false],
  'Chain Interaction/remote_chain_head_continue': ['chainInteraction', 'chainHeadContinue', false],
  'Chain Interaction/remote_chain_head_stop_operation': ['chainInteraction', 'chainHeadStopOperation', false],
  'Chain Interaction/remote_chain_spec_genesis_hash': ['chainInteraction', 'chainSpecGenesisHash', false],
  'Chain Interaction/remote_chain_spec_chain_name': ['chainInteraction', 'chainSpecChainName', false],
  'Chain Interaction/remote_chain_spec_properties': ['chainInteraction', 'chainSpecProperties', false],
  'Chain Interaction/remote_chain_transaction_broadcast': ['chainInteraction', 'chainTransactionBroadcast', false],
  'Chain Interaction/remote_chain_transaction_stop': ['chainInteraction', 'chainTransactionStop', false],

  // Payment
  'Payment/host_payment_balance_subscribe': ['payment', 'paymentBalanceSubscribe', true],
  'Payment/host_payment_top_up': ['payment', 'paymentTopUp', false],
  'Payment/host_payment_request': ['payment', 'paymentRequest', false],
  'Payment/host_payment_status_subscribe': ['payment', 'paymentStatusSubscribe', true],

  // Entropy
  'Entropy Derivation/host_derive_entropy': ['entropyDerivation', 'deriveEntropy', false],
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
  if (typeof fn !== 'function') return null;
  return { fn: fn as ClientMethod, thisArg: serviceObj };
}

export function getMethodBinding(
  service: string,
  method: string,
): MethodBinding | null {
  const entry = methodMap[`${service}/${method}`];
  if (!entry) return null;

  const [serviceField, methodName, isStream] = entry;
  const client = getClient();
  const resolved = resolveClientMethod(client, serviceField, methodName);
  if (!resolved) return null;
  const { fn, thisArg } = resolved;

  if (isStream) {
    return {
      isStream: true,
      subscribe(req, onEvent, onEnd) {
        const args = subscriptionArgs(req, onEvent);
        const unsubscribe = fn.apply(thisArg, args) as () => void;
        // The new client does not surface subscription end events. Wire onEnd
        // to the close callback by also subscribing to the transport's close
        // signal? The provider already handles that at the connection level.
        // For per-subscription end notification, the host emits an Interrupt
        // frame; the new client passes that to the optional `onInterrupt`
        // callback for some subscriptions. Where it is unavailable we treat
        // unsubscribe as the only end signal.
        return {
          unsubscribe: () => {
            try {
              unsubscribe();
            } catch {
              // Transport cleanup errors are benign.
            }
            onEnd();
          },
        };
      },
    };
  }

  return {
    isStream: false,
    async call(req) {
      const args = unaryArgs(req);
      try {
        const result = (await fn.apply(thisArg, args)) as
          | { success: true; value: unknown }
          | { success: false; value: unknown };
        return { ok: result.success, data: result.value };
      } catch (error) {
        return {
          ok: false,
          data: { message: error instanceof Error ? error.message : String(error) },
        };
      }
    },
  };
}

// The generated client takes inner request values directly. Methods with one
// parameter expect a single argument; methods with multiple parameters (e.g.
// `accountCreateProof(productAccountId, ringLocation, context)`) take a tuple
// supplied as a JSON array. `noParams` methods get an empty arg list.
function unaryArgs(req: unknown): unknown[] {
  if (req === null || req === undefined) return [];
  if (Array.isArray(req)) return req;
  return [req];
}

function subscriptionArgs(req: unknown, callback: (data: unknown) => void): unknown[] {
  return [...unaryArgs(req), callback];
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
            '0x' +
            Array.from(v)
              .map(b => b.toString(16).padStart(2, '0'))
              .join(''),
        };
      }
      if (typeof v === 'bigint') return v.toString() + 'n';
      return v;
    },
    2,
  );
}
