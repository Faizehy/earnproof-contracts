# Add Proof Subject Pseudonym Commitments

Closes #161

## Summary

Adds an optional fixed-size subject pseudonym commitment to proof registration without changing the serialized `ProofRecord`. Nonzero values are stored as opaque 32-byte sidecar data; an all-zero commitment explicitly means absent and creates no sidecar. Legacy registration remains compatible and returns `None` for pseudonyms not recorded.

A shared helper constructs commitments as a versioned SHA-256 over a purpose domain, issuer account StrKey, and fixed-size pseudonym. Purpose and issuer separation prevent accidental reuse across domains or issuers. Neither raw subject identities nor wallet addresses are included in the commitment input or stored by the new flow.

## Validation

Passed:

- `cargo test -p earnproof-shared --lib` — 10 passed.
- `cargo check -p proof-registry --lib` — passed.
- `cargo clippy -p earnproof-shared --all-targets --all-features -- -D warnings` — passed.
- `cargo clippy -p proof-registry --lib -- -D warnings` — passed.
- `rustfmt --edition 2021 --check packages/shared/src/lib.rs packages/shared/src/storage_namespaces.rs contracts/proof-registry/src/lib.rs tests/storage-keys/src/support.rs tests/storage-keys/src/encoding.rs fuzz/fuzz_targets/fuzz_proof_context.rs` — passed.
- `node --experimental-strip-types tests/fixtures/encoding/example.ts` — passed; issuer/domain-separated pseudonym outputs match the golden vectors.
- `node -e 'const fs=require("node:fs"); const v=JSON.parse(fs.readFileSync("tests/fixtures/encoding/vectors.json","utf8")); JSON.parse(fs.readFileSync("tests/compatibility/goldens/proof-registry.abi.json","utf8")); if(v.subjectPseudonymV1.commitment!=="5ecb42daec707c8a896025a5e7c7b32b1f85a2f7ccc95a3afeb4dea1a4ea10bd") throw Error("vector mismatch");'` — passed.
- `cargo check -p earnproof-fuzz --bin fuzz_proof_context` — passed.
- `cargo run -p earnproof-fuzz --bin fuzz_proof_context -- -runs=1000` — 1,000 executions completed without a panic. Direct execution reports that coverage instrumentation is unavailable.
- `git diff --check` — passed.

Blocked by existing repository issues:

- `cargo fmt --check` — fails on an unclosed delimiter in `contracts/issuer-registry/src/lib.rs`, formatting diffs in untouched authorization tests, and a parse error in `tests/events/src/compatibility.rs`.
- `cargo clippy --all-targets --all-features -- -D warnings` — blocked by the issuer-registry unclosed delimiter and a duplicate `metadata_hash` initializer in the existing `fuzz_issuer_record_decode` target.
- `cargo test --workspace` — blocked by the same issuer-registry unclosed delimiter. `cargo test -p proof-registry --lib` and `cargo test -p storage-key-tests` are also blocked while compiling that dependency.

## Coverage

Adds known-vector tests, same-pseudonym issuer/domain separation checks, zero-sentinel and domain-boundary tests, pseudonym-only and context-aware registration checks, issuer authorization coverage, legacy absence behavior, opaque sidecar storage assertions, and pseudonym key-encoding coverage.
