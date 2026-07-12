/// <reference path="../runner.ts" />
export {};

const login = await truapi.account.requestLogin({ reason: undefined });
if (!login.isOk() || login.value !== "AlreadyConnected") {
  throw new Error(`requestLogin failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`);
}

const account = host.productAccount();
const accountResult = await truapi.account.getAccount({ productAccountId: account });
accountResult.match(
  (value) => console.log(`ACCOUNT ${value.account.publicKey.slice(0, 18)}`),
  (error) => {
    throw new Error(`getAccount failed: ${JSON.stringify(error)}`);
  },
);

const signatureResult = await truapi.signing.signRaw({
  account,
  payload: { tag: "Bytes", value: { bytes: "0xdeadbeef" } },
});
signatureResult.match(
  (value) => console.log(`SIGNATURE ${value.signature.slice(0, 18)}`),
  (error) => {
    throw new Error(`signRaw failed: ${JSON.stringify(error)}`);
  },
);

console.log("SIGNING_SMOKE_OK");
