# Backend Encoding Vectors

The canonical fixture is [vectors.json](../tests/fixtures/encoding/vectors.json); the tab-separated form is consumed by two Rust test modules:

- `tests/encoding/src/lib.rs::sha256_vectors_match_published_hex` proves the fixture's hex digests are genuinely SHA-256 of their documented UTF-8 source strings — that following the rule below produces the published bytes.
- `tests/encoding/src/contract_compatibility.rs` proves those exact bytes are accepted by the REAL `issuer-registry`/`proof-registry` contracts when passed as `BytesN<32>`, and that the events those contracts emit in response carry the identical bytes back out — a full backend → contract → event round-trip, not just a standalone hashing check.

## Rules

- Text is encoded as UTF-8, with no normalization, trimming, newline, prefix removal, or case folding. Unicode is therefore hashed by its UTF-8 bytes. Empty text and malformed UTF-8 are rejected by backend validation; the contracts accept only already-sized `BytesN<32>` values.
- `proof_id_hash`, `commitment_hash`, `issuer_id_hash`, and `metadata_hash` are SHA-256 digests represented as exactly 32 bytes. The hex form is lowercase for transport only.
- `schema_version` is an unsigned 32-bit integer encoded big-endian when serialized outside Soroban. `expiration` is an unsigned 64-bit ledger timestamp, also big-endian. No signed, little-endian, truncated, or overflowing value is valid.
- `BytesN<32>` is the digest bytes, not the ASCII bytes of its hexadecimal display.
- A disclosure policy is serialized as RFC 8785 canonical JSON, encoded as UTF-8 without a byte-order mark or trailing newline, then hashed with SHA-256. Object keys are canonicalized by the RFC; array order is significant. Reject duplicate object keys and non-JSON values before canonicalization. `disclosure-policy-basic` and `disclosure-policy-eligibility` in `vectors.tsv` are normative byte-for-byte examples.
- The all-zero disclosure-policy hash means “no policy commitment supplied” and is accepted only by the legacy registration endpoints. The policy-aware registration endpoints reject that sentinel. The contract stores only the fixed-size hash, never policy bytes.

## Proof commitment algorithms

The algorithm identifier is stored with each proof and is never inferred from
the current protocol policy:

| ID | Binding name | Commitment bytes |
| --- | --- | --- |
| `0` | `LegacySha256` | `SHA-256(canonical_payload_bytes)`; preserves the existing unprefixed interpretation. |
| `1` | `Sha256CanonicalV1` | `SHA-256("earnproof:proof-commitment:v1" || 0x00 || canonical_payload_bytes)`. |

The schema codec defines `canonical_payload_bytes`; the proof registry stores
the caller-supplied digest and algorithm ID but never stores or hashes the raw
credential. The `tests/encoding` known vectors pin both byte constructions.
Protocol governance may retire an ID for new registrations without changing
the interpretation of existing records.

Schema policy uses stable unsigned 32-bit proof-type IDs. ID `0` is reserved
for calls through the original registration functions. A schema policy is a
non-empty list of at most 16 IDs and a maximum validity duration from 1 through
3,153,600,000 seconds. Updating a policy requires deprecating that schema
version first, then approving it again; each proof retains its registration
snapshot.

The fixtures contain synthetic values only. They must never be updated with wallets, credentials, secrets, deployment identifiers, income, or payment history. Run `cargo test -p encoding-vector-tests` after changing them and update the independent TypeScript example in `tests/fixtures/encoding/example.ts`.

## Proof network and asset context (version 1)

Context-aware proof registration binds the caller's claim ID and claim
commitment to the active Stellar network and one explicitly encoded asset.
Network passphrases are 1-128 visible ASCII bytes, cannot start or end with a
space, and must hash to the current ledger's network ID. The contract stores
only commitments; it never stores a payment address, transaction, amount, or
other payment data.

The asset is a tagged choice:

- Native XLM: the literal ASCII bytes `native`.
- Issued asset: `issued`, a NUL byte, a one-byte code length, the exact
	case-sensitive ASCII alphanumeric code (1-12 bytes), and the issuer's
	canonical Stellar account StrKey. Contract addresses are not valid issuers.

The preimages are concatenated exactly as shown, with no additional lengths or
normalization except the issued-code length byte:

```text
network = SHA256("earnproof.network.v1\0" || passphrase_ascii)
asset = SHA256("earnproof.asset.v1\0" || asset_encoding)
context = SHA256("earnproof.proof-context.v1\0" || claim_commitment_32 || network_32 || asset_32)
record_id = SHA256("earnproof.proof-record.v1\0" || caller_claim_id_32 || context_32)
```

The context-aware `register_proof_with_context` method returns `record_id`; use
it for subsequent proof queries. Set its optional payload field to store a
bounded auxiliary payload alongside the context. Consequently, the same caller claim ID and claim commitment produce
different record IDs when either network or asset changes. Repeating the same
claim/network/asset produces the same ID and is rejected by the existing
duplicate-record guard. Golden values are in `proofContextV1` in
[`vectors.json`](../tests/fixtures/encoding/vectors.json), mirrored in
[`vectors.tsv`](../tests/fixtures/encoding/vectors.tsv) and independently
computed in [`example.ts`](../tests/fixtures/encoding/example.ts).

## Subject pseudonym commitment (version 1)

Backends may bind an optional opaque subject pseudonym during context-aware or
pseudonym-only registration. Compute:

```text
subject_pseudonym_commitment = SHA256(
	"earnproof.subject-pseudonym.v1\0" ||
	domain_length_u8 || domain_ascii ||
	issuer_account_strkey_ascii ||
	subject_pseudonym_32
)
```

The purpose domain is 1-64 visible ASCII bytes with no whitespace. The issuer
must be a Stellar account address. Include a new purpose domain when the same
subject pseudonym is used for a different application; issuer and domain are
both bound into the commitment, so matching pseudonym bytes cannot be reused
across issuers or purposes accidentally. The wallet address and subject ID
are never registration arguments or stored values. Only the resulting
32-byte commitment is passed to the contract.

An all-zero `BytesN<32>` is the explicit optionality sentinel: it means no
pseudonym was supplied, and the contract stores no pseudonym sidecar. Legacy
registration entrypoints likewise create no pseudonym value. The
`ProofRecord` serialization is unchanged; query the optional commitment
through `get_proof_pseudonym_commitment`. The known vector and cross-domain /
cross-issuer outputs are in `subjectPseudonymV1` in
[`vectors.json`](../tests/fixtures/encoding/vectors.json).