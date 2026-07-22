// Establishes the host session before the main driver runs: the pairing host
// emits its deeplink on the first requestLogin, and the runtime holds the
// resulting session for every later product connection. Mirrors how the dotli
// e2e baseline signs in before driving Diagnosis.
const result = await truapi.account.requestLogin();
console.log("PRELOGIN_OK", JSON.stringify(result).slice(0, 160));
