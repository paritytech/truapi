/// <reference path="../runner.ts" />
export {};

const PEOPLE_COLLECTION_ID =
  "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";
const PEOPLE_GENESIS =
  "0xc5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5";
const index = { tag: "Index" as const, value: 0 };
const keyHandle = {
  dotNsIdentifier: host.productId,
  derivationIndex: index,
};
const context = { productId: host.productId, suffix: index };
const ringLocation = {
  chainId: PEOPLE_GENESIS as `0x${string}`,
  junctions: [
    {
      tag: "CollectionId" as const,
      value: PEOPLE_COLLECTION_ID as `0x${string}`,
    },
  ],
};

const login = await truapi.account.requestLogin({ reason: undefined });
if (
  !login.isOk() ||
  (login.value !== "Success" && login.value !== "AlreadyConnected")
) {
  throw new Error(
    `requestLogin failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`,
  );
}

const registration = await truapi.account.registerRingVrfKey({
  index,
  ring: ringLocation,
});
if (!registration.isOk()) {
  throw new Error(
    `registerRingVrfKey failed: ${JSON.stringify(registration.error)}`,
  );
}

const listed = await truapi.account.listRingVrfKeys({
  owner: host.productId,
  disclosure: "PublicKey",
});
if (!listed.isOk()) {
  throw new Error(`listRingVrfKeys failed: ${JSON.stringify(listed.error)}`);
}
const entry = listed.value.find(
  (candidate) =>
    candidate.handle.dotNsIdentifier === host.productId &&
    candidate.handle.derivationIndex.tag === "Left" &&
    candidate.handle.derivationIndex.value === index.value,
);
if (!entry || entry.publicKey !== registration.value) {
  throw new Error("registered key was not returned by listRingVrfKeys");
}

const aliasResult = await truapi.account.getAccountAlias({
  keyHandle,
  context,
  ringLocation,
});
if (!aliasResult.isOk()) {
  throw new Error(
    `getAccountAlias failed: ${JSON.stringify(aliasResult.error)}`,
  );
}

const proofResult = await truapi.account.createAccountProof({
  keyHandle,
  context,
  ringLocation,
  message: "0x48656c6c6f",
});
if (
  !proofResult.isErr() ||
  proofResult.error.tag !== "Domain" ||
  proofResult.error.value.tag !== "V1" ||
  proofResult.error.value.value.tag !== "NotMember"
) {
  throw new Error(
    `createAccountProof did not report NotMember for the fresh key: ${JSON.stringify(proofResult)}`,
  );
}

const signature = await truapi.account.ringVrfSign({
  keyHandle,
  message: "0x48656c6c6f",
});
if (!signature.isOk() || signature.value.length !== 130) {
  throw new Error(`ringVrfSign failed: ${JSON.stringify(signature)}`);
}

console.log(
  `RING_VRF_OK publicKey=${registration.value} alias=${aliasResult.value.alias} signatureBytes=64`,
);
