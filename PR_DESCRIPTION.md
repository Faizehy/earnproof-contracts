# Threshold Approval Governance for Critical Protocol Changes

Closes #203

## Summary

Adds optional, contract-local threshold approval for critical schema policy, dependency replacement, and approval-policy changes. Proposal commitments bind the contract instance, action category, canonical parameters, and a monotonic nonce. Proposals snapshot the signer threshold, expire after 518,400 ledgers, support admin cancellation, and are consumed only after successful execution. Emergency pause, unpause, and scoped-pause authority remains immediate.

## Governed Actions

- `ProtocolConfig`: schema approval/deprecation, schema payload limits, and approval-policy updates.
- `ProofRegistry`: issuer-registry/protocol-config replacement and approval-policy updates. Existing address and interface compatibility checks still run at execution.

## Validation

Passed:

- `cargo test -p earnproof-shared --lib` — 4 passed.
- `cargo test -p protocol-config --lib` — 59 passed.
- `cargo check -p proof-registry --lib` — passed.
- `cargo clippy -p protocol-config --lib -- -D warnings` — passed.
- `cargo clippy -p proof-registry --lib -- -D warnings` — passed.
- `rustfmt --edition 2021 --check packages/shared/src/lib.rs packages/shared/src/error_catalog.rs contracts/protocol-config/src/lib.rs contracts/proof-registry/src/lib.rs` — passed.
- `git diff --check` — passed.

Blocked by existing repository issues:

- `cargo fmt --check` — fails on formatting in untouched authorization tests and a parse error in `tests/events/src/compatibility.rs`.
- `cargo clippy --all-targets --all-features -- -D warnings` — fails because `contracts/issuer-registry/src/lib.rs` has an unclosed delimiter.
- `cargo test --workspace` — fails on the same issuer-registry parse error. Consequently `cargo test -p proof-registry --lib` and the error-catalog regeneration test are also blocked by issuer-registry compilation errors.
- No governance-specific fuzz target exists; `cargo fuzz` is not installed.

## Coverage

Adds tests for mixed signers, unauthorized and duplicate signers, action-category/parameter binding, cancellation, expiry, replay, failed execution preservation, policy validation, dependency compatibility, contract-scoped proposals, and immediate emergency pause behavior.
