import { existsSync, readFileSync } from "node:fs";
import {
  PASEO_NEXT_V2_INDIVIDUALITY,
  type ProductAccountId,
  type RegisteredRingVrfKey,
  type RingLocation,
  type TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import {
  actionLines,
  approvalLines,
  newLinesSince,
} from "./auto-signing-e2e.ts";
import type { DiagnosisRow } from "./diagnosis.ts";

const PEOPLE_COLLECTION_ID =
  "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652020202020";
const PEOPLE_LITE_COLLECTION_ID =
  "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";
const ACCOUNT_ACCESS_ACTION = "access another product account";
const PROOF_ACTION = "create account proof";

// These locate the real People collections while intentionally omitting the
// optional pallet junction. They therefore exercise ring operations without
// registering the battery product as an exact-match internal People provider.
const TEST_PEOPLE_LITE_RING: RingLocation = {
  chainId: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
  junctions: [{ tag: "CollectionId", value: PEOPLE_LITE_COLLECTION_ID }],
};

const TEST_PEOPLE_RING: RingLocation = {
  chainId: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
  junctions: [{ tag: "CollectionId", value: PEOPLE_COLLECTION_ID }],
};

function stringify(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function okValue<T>(result: unknown, operation: string): T {
  const candidate = result as {
    isOk(): boolean;
    value: T;
    error: unknown;
  };
  if (!candidate.isOk()) {
    throw new Error(`${operation} failed: ${stringify(candidate.error)}`);
  }
  return candidate.value;
}

function expectDomainError(
  result: unknown,
  expected: string,
  operation: string,
): void {
  const candidate = result as {
    isErr(): boolean;
    error: unknown;
  };
  if (!candidate.isErr()) {
    throw new Error(`${operation} unexpectedly succeeded`);
  }
  const error = candidate.error as {
    tag?: string;
    value?: { tag?: string; value?: { tag?: string } };
  };
  const actual =
    error.tag === "Domain" && error.value?.tag === "V1"
      ? error.value.value?.tag
      : undefined;
  if (actual !== expected) {
    throw new Error(
      `${operation} returned ${actual ?? "a non-domain error"}, expected ${expected}: ${stringify(candidate.error)}`,
    );
  }
}

function requireHexBytes(value: unknown, bytes: number, label: string): string {
  if (
    typeof value !== "string" ||
    !new RegExp(`^0x[0-9a-fA-F]{${bytes * 2}}$`).test(value)
  ) {
    throw new Error(
      `${label} is not a ${bytes}-byte hex value: ${stringify(value)}`,
    );
  }
  return value;
}

function sameIndex(left: unknown, right: unknown): boolean {
  return stringify(left) === stringify(right);
}

function findEntry(
  entries: RegisteredRingVrfKey[],
  handle: ProductAccountId,
): RegisteredRingVrfKey | undefined {
  return entries.find(
    (entry) =>
      entry.handle.dotNsIdentifier === handle.dotNsIdentifier &&
      sameIndex(entry.handle.derivationIndex, handle.derivationIndex),
  );
}

function hasRing(entry: RegisteredRingVrfKey, ring: RingLocation): boolean {
  return entry.rings.some(
    (candidate) => stringify(candidate) === stringify(ring),
  );
}

function transcriptReader(path: string): () => string[] {
  return () =>
    existsSync(path) ? approvalLines(readFileSync(path, "utf8")) : [];
}

function finish(
  methodName: string,
  startedAt: number,
  status: DiagnosisRow["status"],
  output: string,
): DiagnosisRow {
  return {
    id: `Account/${methodName}`,
    serviceName: "Account",
    methodName,
    status,
    output,
    durationMs: Math.round(performance.now() - startedAt),
  };
}

/** Exercise the RFC-0024 registry and authorization contract before examples run. */
export async function runRingVrfRegistryE2e(
  client: TrUApiClient,
  productId: string,
  approvalsLogPath: string | undefined,
): Promise<DiagnosisRow> {
  const startedAt = performance.now();
  if (!approvalsLogPath) {
    return finish(
      "ring_vrf_registry_e2e",
      startedAt,
      "fail",
      "TRUAPI_APPROVALS_LOG not set; cannot verify RFC-0024 prompt behavior",
    );
  }
  const readTranscript = transcriptReader(approvalsLogPath);
  const index = { tag: "Left" as const, value: 0 };
  const handle: ProductAccountId = {
    dotNsIdentifier: productId,
    derivationIndex: index,
  };
  const context = {
    productId,
    suffix: { tag: "Left" as const, value: 24 },
  };

  try {
    const unregistered = await client.account.ringVrfSign({
      keyHandle: {
        dotNsIdentifier: productId,
        derivationIndex: { tag: "Left", value: 4_242 },
      },
      message: "0x756e72656769737465726564",
    });
    expectDomainError(
      unregistered,
      "KeyNotRegistered",
      "unregistered ring_vrf_sign",
    );

    const registered = okValue<string>(
      await client.account.registerRingVrfKey({
        index,
        ring: TEST_PEOPLE_LITE_RING,
      }),
      "register_ring_vrf_key",
    );
    requireHexBytes(registered, 32, "registered public key");
    const repeated = okValue<string>(
      await client.account.registerRingVrfKey({
        index,
        ring: TEST_PEOPLE_LITE_RING,
      }),
      "idempotent register_ring_vrf_key",
    );
    if (repeated !== registered) {
      throw new Error(
        "idempotent registration returned a different public key",
      );
    }
    const secondRing = okValue<string>(
      await client.account.registerRingVrfKey({ index, ring: TEST_PEOPLE_RING }),
      "multi-ring register_ring_vrf_key",
    );
    if (secondRing !== registered) {
      throw new Error(
        "multi-ring registration returned a different public key",
      );
    }

    const publicEntries = okValue<RegisteredRingVrfKey[]>(
      await client.account.listRingVrfKeys({
        owner: productId,
        disclosure: "PublicKey",
      }),
      "owned public list_ring_vrf_keys",
    );
    const publicEntry = findEntry(publicEntries, handle);
    if (!publicEntry || publicEntry.publicKey !== registered) {
      throw new Error("owned public listing omitted the registered public key");
    }
    if (
      !hasRing(publicEntry, TEST_PEOPLE_LITE_RING) ||
      !hasRing(publicEntry, TEST_PEOPLE_RING)
    ) {
      throw new Error(
        "multi-ring registration was not preserved by the registry",
      );
    }

    const anonymousEntries = okValue<RegisteredRingVrfKey[]>(
      await client.account.listRingVrfKeys({
        owner: productId,
        disclosure: "Anonymized",
      }),
      "owned anonymized list_ring_vrf_keys",
    );
    const anonymousEntry = findEntry(anonymousEntries, handle);
    if (!anonymousEntry || anonymousEntry.publicKey !== undefined) {
      throw new Error(
        "anonymized listing disclosed or omitted the registered key",
      );
    }

    const alias = okValue<{ context: string; alias: string }>(
      await client.account.getAccountAlias({
        keyHandle: handle,
        context,
        ringLocation: TEST_PEOPLE_LITE_RING,
      }),
      "owned get_account_alias",
    );
    requireHexBytes(alias.context, 32, "alias context");
    if (!alias.alias.startsWith("0x") || alias.alias.length <= 2) {
      throw new Error(
        `get_account_alias returned an empty alias: ${stringify(alias)}`,
      );
    }

    const proof = await client.account.createAccountProof({
      keyHandle: handle,
      context,
      ringLocation: TEST_PEOPLE_LITE_RING,
      message: "0x7266633234",
    });
    expectDomainError(proof, "NotMember", "owned create_account_proof");

    const structurallyDifferentRing: RingLocation = {
      chainId: TEST_PEOPLE_LITE_RING.chainId,
      junctions: [
        { tag: "PalletInstance", value: 67 },
        ...TEST_PEOPLE_LITE_RING.junctions,
      ],
    };
    const wrongRingAlias = await client.account.getAccountAlias({
      keyHandle: handle,
      context,
      ringLocation: {
        chainId: TEST_PEOPLE_LITE_RING.chainId,
        junctions: [],
      },
    });
    expectDomainError(
      wrongRingAlias,
      "KeyNotInRing",
      "undeclared invalid ring alias",
    );
    const wrongRing = await client.account.createAccountProof({
      keyHandle: handle,
      context,
      ringLocation: structurallyDifferentRing,
      message: "0x7266633234",
    });
    expectDomainError(
      wrongRing,
      "KeyNotInRing",
      "structurally different ring proof",
    );

    const signature = okValue<string>(
      await client.account.ringVrfSign({
        keyHandle: handle,
        message: "0x72666332342072696e6720767266207369676e6174757265",
      }),
      "owned ring_vrf_sign",
    );
    requireHexBytes(signature, 64, "ring VRF signature");

    const foreignOwner = `rfc24${Date.now().toString(36)}.dot`;
    const beforeForeignRead = readTranscript();
    const foreignAnonymous = okValue<RegisteredRingVrfKey[]>(
      await client.account.listRingVrfKeys({
        owner: foreignOwner,
        disclosure: "Anonymized",
      }),
      "foreign anonymized list_ring_vrf_keys",
    );
    if (foreignAnonymous.length !== 0) {
      throw new Error("fresh foreign owner unexpectedly had registry entries");
    }
    const readPrompts = actionLines(
      newLinesSince(beforeForeignRead, readTranscript()),
      ACCOUNT_ACCESS_ACTION,
    );
    if (readPrompts.length !== 1) {
      throw new Error(
        `foreign listing produced ${readPrompts.length} account-access prompts, expected one`,
      );
    }
    const beforeSecondRead = readTranscript();
    okValue<RegisteredRingVrfKey[]>(
      await client.account.listRingVrfKeys({
        owner: foreignOwner,
        disclosure: "PublicKey",
      }),
      "foreign public list_ring_vrf_keys",
    );
    if (
      actionLines(
        newLinesSince(beforeSecondRead, readTranscript()),
        ACCOUNT_ACCESS_ACTION,
      ).length !== 0
    ) {
      throw new Error(
        "persisted foreign account access prompted more than once",
      );
    }

    const foreignHandle: ProductAccountId = {
      dotNsIdentifier: foreignOwner,
      derivationIndex: { tag: "Left", value: 0 },
    };
    const foreignAlias = await client.account.getAccountAlias({
      keyHandle: foreignHandle,
      context,
      ringLocation: TEST_PEOPLE_LITE_RING,
    });
    expectDomainError(
      foreignAlias,
      "KeyNotRegistered",
      "granted foreign alias read",
    );

    const beforeBearerCalls = readTranscript();
    const foreignProof = await client.account.createAccountProof({
      keyHandle: foreignHandle,
      context,
      ringLocation: TEST_PEOPLE_LITE_RING,
      message: "0x6e6f2070726f6d7074",
    });
    expectDomainError(
      foreignProof,
      "NotAllowlisted",
      "foreign create_account_proof",
    );
    const foreignSignature = await client.account.ringVrfSign({
      keyHandle: foreignHandle,
      message: "0x6e6f2070726f6d7074",
    });
    expectDomainError(
      foreignSignature,
      "NotAllowlisted",
      "foreign ring_vrf_sign",
    );
    const bearerWindow = newLinesSince(beforeBearerCalls, readTranscript());
    if (
      actionLines(bearerWindow, PROOF_ACTION).length !== 0 ||
      actionLines(bearerWindow, ACCOUNT_ACCESS_ACTION).length !== 0
    ) {
      throw new Error(
        `foreign bearer-token calls consulted a prompt: ${bearerWindow.join("; ")}`,
      );
    }

    return finish(
      "ring_vrf_registry_e2e",
      startedAt,
      "pass",
      "registry, exact rings, disclosure, alias/proof/signing, and foreign authorization verified",
    );
  } catch (error) {
    return finish(
      "ring_vrf_registry_e2e",
      startedAt,
      "fail",
      error instanceof Error ? error.message : String(error),
    );
  }
}

/** Verify an AutoSigning host can create and immediately use a new registry entry. */
export async function runAutoSigningRingVrfE2e(
  client: TrUApiClient,
  productId: string,
): Promise<DiagnosisRow> {
  const startedAt = performance.now();
  const index = {
    tag: "Right" as const,
    value:
      "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f" as const,
  };
  const handle: ProductAccountId = {
    dotNsIdentifier: productId,
    derivationIndex: index,
  };

  try {
    const publicKey = okValue<string>(
      await client.account.registerRingVrfKey({
        index,
        ring: TEST_PEOPLE_LITE_RING,
      }),
      "AutoSigning register_ring_vrf_key",
    );
    requireHexBytes(publicKey, 32, "AutoSigning ring VRF public key");
    const entries = okValue<RegisteredRingVrfKey[]>(
      await client.account.listRingVrfKeys({
        owner: productId,
        disclosure: "PublicKey",
      }),
      "post-AutoSigning list_ring_vrf_keys",
    );
    const entry = findEntry(entries, handle);
    if (
      !entry ||
      entry.publicKey !== publicKey ||
      !hasRing(entry, TEST_PEOPLE_LITE_RING)
    ) {
      throw new Error(
        "AutoSigning registration was not immediately visible in the registry",
      );
    }
    const signature = okValue<string>(
      await client.account.ringVrfSign({
        keyHandle: handle,
        message: "0x6175746f7369676e696e672072696e6720767266",
      }),
      "post-AutoSigning ring_vrf_sign",
    );
    requireHexBytes(signature, 64, "post-AutoSigning ring VRF signature");
    return finish(
      "auto_signing_ring_vrf_e2e",
      startedAt,
      "pass",
      "AutoSigning registration was immediately listed and usable for direct signing",
    );
  } catch (error) {
    return finish(
      "auto_signing_ring_vrf_e2e",
      startedAt,
      "fail",
      error instanceof Error ? error.message : String(error),
    );
  }
}
