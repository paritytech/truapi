// Curated signer battery, as a product script for the pairing host.
//
// Run via: truapi-host pairing-host --product-id <p> --script js/scripts/battery.ts
// The runner injects `truapi` (the @parity/truapi client, scoped to the product)
// and `host` (helpers). Logs in with the paired signing host, exercises the
// signer-backed methods the playground diagnosis covers, and throws on any
// failure so the host command exits non-zero.
import type { HostContext } from "../runner.ts";

const GENESIS_HASH = `0x${"11".repeat(32)}` as const;

interface Case {
  name: string;
  ok: boolean;
  detail: string;
}

export default async function run(host: HostContext) {
  const account = host.productAccount();
  const results: Case[] = [];

  const record = async (name: string, fn: () => Promise<{ ok: boolean; detail: string }>) => {
    try {
      results.push({ name, ...(await fn()) });
    } catch (error) {
      results.push({ name, ok: false, detail: `threw: ${String(error)}` });
    }
  };

  const login = await truapi.account.requestLogin({ reason: undefined });
  const loginOk = login.isOk() && login.value === "Success";
  results.push({
    name: "account.requestLogin",
    ok: loginOk,
    detail: login.isOk() ? String(login.value) : JSON.stringify(login.error),
  });
  if (!loginOk) {
    report(results);
    throw new Error("login did not succeed");
  }

  await record("account.getAccount", async () => {
    const result = await truapi.account.getAccount({ productAccountId: account });
    return result.match(
      (value) => ({
        ok: value.account.publicKey.startsWith("0x") && value.account.publicKey.length > 4,
        detail: value.account.publicKey.slice(0, 18),
      }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  await record("signing.signRaw(bytes)", async () => {
    const result = await truapi.signing.signRaw({
      account,
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
    const result = await truapi.signing.signRaw({
      account,
      payload: { tag: "Payload", value: { payload: "hello from the headless battery" } },
    });
    return result.match(
      (value) => ({ ok: value.signature.startsWith("0x"), detail: value.signature.slice(0, 18) }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  await record("signing.signPayload", async () => {
    const result = await truapi.signing.signPayload({
      account,
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
    const result = await truapi.signing.createTransaction({
      signer: account,
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
    const result = await truapi.entropy.derive({ context: "0x6d792d6b6579" });
    return result.match(
      (value) => ({ ok: value.entropy.startsWith("0x"), detail: value.entropy.slice(0, 18) }),
      (error) => ({ ok: false, detail: JSON.stringify(error) }),
    );
  });

  report(results);
  const failures = results.filter((r) => !r.ok);
  if (failures.length > 0) {
    throw new Error(`GATE FAILED: ${failures.map((r) => r.name).join(", ")}`);
  }
  host.log(`GATE PASSED: ${results.length} signer-critical cases`);
}

function report(results: Case[]) {
  console.log("\n=== Headless host signer battery ===");
  for (const r of results) {
    console.log(`${r.ok ? "PASS" : "FAIL"}  ${r.name.padEnd(28)} ${r.detail}`);
  }
  const pass = results.filter((r) => r.ok).length;
  console.log(`--------------------------------\n${pass}/${results.length} passed`);
}
