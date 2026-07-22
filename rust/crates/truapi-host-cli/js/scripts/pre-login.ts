// Establishes the host session before the main driver runs: the pairing host
// emits its deeplink on the first requestLogin, and the runtime holds the
// resulting session for every later product connection. Mirrors how the dotli
// e2e baseline signs in before driving Diagnosis.
const login = await truapi.account.requestLogin({ reason: undefined });
if (!(login.isOk() && login.value === "Success")) {
  throw new Error(`pre-login failed: ${login.isOk() ? String(login.value) : JSON.stringify(login.error)}`);
}
console.log("PRELOGIN_OK");
