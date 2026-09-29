# Commit Network and Asset Identifiers in Proof Records

Closes #160

## Summary

Adds context-aware proof registration that binds a claim commitment and caller claim ID to the active Stellar network and a canonical asset policy. Asset choices are explicitly tagged as native XLM or an issued asset with a case-sensitive ASCII code and account issuer. Domain-separated network, asset, context, and contextual record-ID hashes are stored; raw passphrases, asset identifiers, and payment data are not.

The serialized `ProofRecord` remains unchanged. Context hashes live in a versioned `ProofContext` sidecar, so pre-existing records remain readable and report `None` for unavailable context. The new registration method returns the derived record ID for subsequent queries. Existing registration APIs remain available for legacy compatibility.

## Validation

Passed:

- `cargo test -p earnproof-shared --lib` — 8 passed.
- `cargo test -p protocol-config --lib` — 59 passed.
- `cargo check -p proof-registry --lib` — passed.
- `cargo clippy -p earnproof-shared --all-targets --all-features -- -D warnings` — passed.
- `cargo clippy -p proof-registry --lib -- -D warnings` — passed.
- `rustfmt --edition 2021 --check packages/shared/src/lib.rs packages/shared/src/storage_namespaces.rs contracts/proof-registry/src/lib.rs tests/storage-keys/src/support.rs tests/storage-keys/src/encoding.rs fuzz/fuzz_targets/fuzz_proof_context.rs` — passed.
- `node --experimental-strip-types tests/fixtures/encoding/example.ts` — passed; TypeScript output matches the seven published context vectors.
- `node -e 'const fs=require("node:fs"); JSON.parse(fs.readFileSync("tests/fixtures/encoding/vectors.json","utf8")); JSON.parse(fs.readFileSync("tests/compatibility/goldens/proof-registry.abi.json","utf8"));'` — passed.
- `cargo run -p earnproof-fuzz --bin fuzz_proof_context -- -runs=1000` — 1,000 executions completed without a panic. This direct run lacks coverage instrumentation.
- `git diff --check` — passed.

Blocked by existing repository issues:

- `cargo fmt --check` — fails on an unclosed delimiter in `contracts/issuer-registry/src/lib.rs`, formatting diffs in untouched authorization tests, and a parse error in `tests/events/src/compatibility.rs`.
- `cargo clippy --all-targets --all-features -- -D warnings` — blocked by the issuer-registry unclosed delimiter and a duplicate `metadata_hash` initializer in the existing `fuzz_issuer_record_decode` target.
- `cargo test --workspace` — blocked by the issuer-registry unclosed delimiter. This also blocks `cargo test -p proof-registry --lib`, `cargo test -p encoding-vector-tests`, and the storage-key integration test package.

## Coverage

Adds shared tests for cross-language commitment vectors, distinct networks and assets, malformed asset IDs, account-only issuers, and input length boundaries. Adds proof-registry tests for context-specific IDs, payload compatibility, legacy records, replay rejection, malformed context atomicity, and issuer authorization. Adds a focused libFuzzer target for passphrase and asset-code validation.
