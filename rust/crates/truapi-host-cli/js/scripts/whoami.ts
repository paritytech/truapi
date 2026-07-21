/// <reference path="../runner.ts" />
export {};

const result = await truapi.account.getUserId();
if (!result.isOk()) {
  throw new Error(`getUserId failed: ${JSON.stringify(result.error)}`);
}

console.log(`WHOAMI ${result.value.primaryUsername}`);
