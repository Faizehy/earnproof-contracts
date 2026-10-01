# Contributing to EarnProof Contracts

This repository contains Soroban contracts for EarnProof issuer status, proof commitments, revocation state, and protocol configuration.

## Setup

```bash
cargo build --workspace
cargo test --workspace
git config core.hooksPath .githooks
```

Run the Git command once per checkout to enable the tracked hooks. The
pre-commit hook runs workspace tests when staged Rust or Cargo files change.
The commit-message hook accepts conventional subjects such as `feat: add proof
search` and `fix(registry): reject inactive issuers`; scopes and issue IDs are
optional.

## Validation

Run these before opening a pull request:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo build --workspace
```

## Contribution Expectations

- Keep changes scoped to the issue being solved.
- Do not commit secret keys, seed phrases, signing material, or deployment keys.
- Add tests for every new contract entry point or state transition.
- Include negative tests for authorization failures and invalid transitions.
- Keep private income data off-chain.
- Update deployment docs and manifests when contract addresses or initialization flows change.
- ///undone but

## Definition of Done

- Acceptance criteria are satisfied.
- Formatting, clippy, tests, and build pass.
- Contract storage, authorization, and event behavior are covered by tests.
- Documentation matches actual on-chain behavior.
- Testnet deployment evidence is updated when deployment behavior changes.
