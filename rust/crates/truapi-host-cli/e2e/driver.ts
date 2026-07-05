// Product-side test battery run against a headless pairing host.
//
// Constructs the real @parity/truapi client over a WebSocket to the pairing
// host's frame endpoint and drives the SSO-class methods the playground
// diagnosis exercises against the signer: login, account lookup, raw signing,
// payload signing, transaction construction, and product entropy. Each method
// is one pass/fail case, mirroring how the playground diagnosis reports.
import {
  createClient,
  createTransport,
  type TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import { wsProvider } from "./ws-provider.ts";

export interface CaseResult {
  name: string;
  ok: boolean;
  detail: string;
}

const PRODUCT_ID = "truapi-playground.dot";
const GENESIS_HASH = `0x${"11".repeat(32)}` as const;

function productAccount(index = 0) {
  return { dotNsIdentifier: PRODUCT_ID, derivationIndex: index };
}

/** Connect a client to the pairing host and return it plus the open signal. */
export function connect(frameUrl: string): {
  client: TrUApiClient;
  opened: Promise<void>;
  dispose: () => void;
} {
  const provider = wsProvider(frameUrl);
  const transport = createTransport(provider);
  const client = createClient(transport);
  return { client, opened: provider.opened, dispose: () => provider.dispose() };
}

/** Begin login; resolves when the host reports the pairing outcome. */
export function beginLogin(client: TrUApiClient) {
  return client.account.requestLogin({ reason: undefined });
}

/** Run the signing battery against an already-paired client. */
export async function runBattery(client: TrUApiClient): Promise<CaseResult[]> {
  const results: CaseResult[] = [];

  const record = async (
    name: string,
    run: () => Promise<{ ok: boolean; detail: string }>,
  ) => {
    try {
      results.push({ name, ...(await run()) });
    } catch (error) {
      results.push({ name, ok: false, detail: `threw: ${String(error)}` });
    }
  };

  await record("account.getAccount", async () => {
    const result = await client.account.getAccount({
      productAccountId: productAccount(),
    });
    return result.match(
      (value) => ({
        ok: value.account.publicKey.startsWith("0x") && value.account.publicKey.length > 4,
        detail: value.account.publicKey.slice(0, 18),
      }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  await record("signing.signRaw(bytes)", async () => {
    const result = await client.signing.signRaw({
      account: productAccount(),
      payload: { tag: "Bytes", value: { bytes: "0xdeadbeef" } },
    });
    return result.match(
      (value) => ({
        ok: value.signature.length === 130 || value.signature.length === 132,
        detail: value.signature.slice(0, 18),
      }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  await record("signing.signRaw(message)", async () => {
    const result = await client.signing.signRaw({
      account: productAccount(),
      payload: { tag: "Payload", value: { payload: "hello from e2e" } },
    });
    return result.match(
      (value) => ({ ok: value.signature.startsWith("0x"), detail: value.signature.slice(0, 18) }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  await record("signing.signPayload", async () => {
    const result = await client.signing.signPayload({
      account: productAccount(),
      payload: {
        blockHash: GENESIS_HASH,
        blockNumber: "0x01",
        era: "0x00",
        genesisHash: GENESIS_HASH,
        method: "0x0400",
        nonce: "0x00",
        specVersion: "0x01000000",
        tip: "0x00",
        transactionVersion: "0x01000000",
        signedExtensions: [],
        version: 4,
        assetId: undefined,
        metadataHash: undefined,
        mode: undefined,
        withSignedTransaction: undefined,
      },
    });
    return result.match(
      (value) => ({ ok: value.signature.startsWith("0x"), detail: value.signature.slice(0, 18) }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  await record("signing.createTransaction", async () => {
    const result = await client.signing.createTransaction({
      signer: productAccount(),
      genesisHash: GENESIS_HASH,
      callData: "0x0000",
      extensions: [{ id: "CheckNonce", extra: "0x04", additionalSigned: "0x" }],
      txExtVersion: 0,
    });
    return result.match(
      (value) => ({
        ok: value.transaction.startsWith("0x") && value.transaction.length > 4,
        detail: `${value.transaction.length} chars`,
      }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  await record("entropy.derive", async () => {
    const result = await client.entropy.derive({ context: "0x6d792d6b6579" });
    return result.match(
      (value) => ({ ok: value.entropy.startsWith("0x"), detail: value.entropy.slice(0, 18) }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  return results;
}
