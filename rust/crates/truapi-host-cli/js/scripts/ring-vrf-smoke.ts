/// <reference path="../runner.ts" />
export {};

const PEOPLE_COLLECTION_ID =
  "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";
const PEOPLE_GENESIS =
  "0xc5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5";
const context = { productId: host.productId, suffix: "0x00" };
const ringLocation = {
  chainId: PEOPLE_GENESIS,
  junctions: [
    { tag: "PalletInstance" as const, value: 67 },
    { tag: "CollectionId" as const, value: PEOPLE_COLLECTION_ID },
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

const aliasResult = await truapi.account.getAccountAlias({ context, ringLocation });
if (!aliasResult.isOk()) {
  throw new Error(`getAccountAlias failed: ${JSON.stringify(aliasResult.error)}`);
}

const proofResult = await truapi.account.createAccountProof({
  context,
  ringLocation,
  message: "0x48656c6c6f",
});
if (!proofResult.isOk()) {
  throw new Error(`createAccountProof failed: ${JSON.stringify(proofResult.error)}`);
}

if (proofResult.value.contextualAlias.alias !== aliasResult.value.alias) {
  throw new Error("alias and proof selected different ring members");
}
if (proofResult.value.contextualAlias.context !== aliasResult.value.context) {
  throw new Error("alias and proof used different context hashes");
}
if (proofResult.value.proof.length <= 2) {
  throw new Error("createAccountProof returned an empty proof");
}

console.log(
  `RING_VRF_OK ring=${proofResult.value.ringIndex} revision=${proofResult.value.ringRevision} proofBytes=${(proofResult.value.proof.length - 2) / 2}`,
);
