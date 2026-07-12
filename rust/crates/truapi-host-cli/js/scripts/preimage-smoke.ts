/// <reference path="../runner.ts" />
// Focused live-chain preimage smoke for debugging Bulletin submission.

const login = await truapi.account.requestLogin({ reason: undefined });
if (!login.isOk() || !["Success", "AlreadyConnected"].includes(String(login.value))) {
  throw new Error(`requestLogin failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`);
}

const bytes = crypto.getRandomValues(new Uint8Array(4));
const value = `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}` as `0x${string}`;
console.log(`PREIMAGE_VALUE ${value}`);

const submitted = await truapi.preimage.submit(value);
console.log(`PREIMAGE_SUBMIT ${JSON.stringify(submitted)}`);
if (!submitted.isOk()) {
  throw new Error(`preimage submit failed: ${JSON.stringify(submitted.error)}`);
}

const item = await new Promise((resolve, reject) => {
  let sub: { unsubscribe: () => void } | undefined;
  sub = truapi.preimage.lookupSubscribe({ request: { key: submitted.value } }).subscribe({
    next(value: unknown) {
      sub?.unsubscribe();
      resolve(value);
    },
    error(error: unknown) {
      reject(error);
    },
  });
});
console.log(`PREIMAGE_LOOKUP ${JSON.stringify(item)}`);
