# Security policy

## Security status

This repository contains reference and proof-of-concept code: the TrUAPI protocol
definitions (Rust traits and types), the codegen toolchain, the generated TypeScript
client, and an interactive playground. It is intended for reference and
experimentation, not as a production-ready artefact.

Unless a specific release states otherwise, this repository has **not** received a full
security audit. Use in production or production-like deployments should only follow an
independent security review of the relevant code, configuration, generated output, and
deployment environment.

Even where no Parity-operated production deployment exists today, this code may be used
by third parties on live networks, or reused in future production contexts once
published.

## Supported versions

Security fixes are provided only for versions, packages, or branches actively maintained
by Parity. Experimental, archived, deprecated, or explicitly-unsupported packages,
examples, or branches may not be triaged unless the issue affects maintained packages,
Parity-operated infrastructure, user funds, private keys, signing flows, or transaction
integrity.

## Bug bounty scope

This repository is **not** in scope for Parity's paid bug bounty programme unless
explicitly listed in the official bounty scope at the time of submission. Reports may
still be reviewed through responsible disclosure, but bounty eligibility applies only
where the affected asset or vulnerability class is explicitly in scope.

## What to report

Report an issue only if it demonstrates realistic impact against one or more of:

- Parity-operated production infrastructure or deployed services;
- maintained SDK packages downstream users are expected to consume;
- user funds or assets;
- private keys, seed phrases, signer flows, or key-management boundaries;
- transaction construction, integrity, or signing intent;
- remote code execution or credential compromise in a realistic deployment.

## Out of scope (unless shown to cause realistic high-impact harm)

Local-development-only issues; demo/example/testnet-only issues; missing security headers
on non-production demos; missing rate limiting in local examples; dependency reports
without a working exploit path or that don't affect shipped packages; hypothetical attack
paths; "this code is unaudited"; documented known limitations; unsafe SDK use contrary to
documented warnings; issues requiring access to internal Parity systems not in scope.

## Reporting a qualifying issue

Do **not** open a public issue for a qualifying vulnerability. Email
**security@parity.io** with:

- the affected repository, package, commit, branch, or release;
- clear reproduction steps and realistic impact;
- whether it affects production infrastructure, maintained packages, user funds, keys,
  signing, or only local/demo/testnet usage;
- any proof of concept, logs, or generated code involved;
- assumptions required for exploitation.

## Researcher expectations

Don't access, modify, or delete data that isn't yours; don't disrupt services; don't
extract keys or secrets beyond what's needed to demonstrate impact safely; don't test
against production systems not in scope; no social engineering or physical attacks; don't
disclose publicly until Parity has had a reasonable opportunity to remediate.

## Safe-use guidance

Before any production or production-like deployment, review at minimum: how
keys/seeds/signers are generated, stored, and destroyed; whether signing prompts display
transaction intent before approval; whether transactions are built against the intended
chain/account/network; whether generated apps default to testnet/devnet; whether storage
assumptions are appropriate; whether any cloud or statement-store data is
public/private/encrypted; whether examples rely on internal/test/unstable endpoints;
whether dependencies are pinned and reviewed; whether generated code has been manually
reviewed before execution; and whether deployment configuration, CORS, auth, admin
routes, logging, and telemetry suit the intended environment.
