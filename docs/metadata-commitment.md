# Issuer metadata commitments

The issuer registry stores two independent 32-byte commitments per issuer:

| Field | Commits to | Meaning |
|---|---|---|
| `metadata_hash` | the canonical metadata **document** | what the metadata says (content) |
| `metadata_uri_hash` | the canonical metadata **document URI** | where the document lives (location) |

Storing them separately lets an off-chain resolver tell a change of *location*
apart from a change of *content*. A pointer that moves without the content
changing bumps only `metadata_uri_hash`; an edit to the document bumps
`metadata_hash`. Both bump `metadata_revision`.

No raw URI or metadata document is ever stored on chain. The contract holds only
the opaque digests.

## Canonical bytes and domain separation

A backend must compute the commitments with an explicit domain-separation
prefix, so a content digest can never be confused with a URI digest even if the
underlying bytes ever coincide:

```
metadata_hash     = SHA-256( "earnproof:issuer-metadata:v1"     || canonical_document_bytes )
metadata_uri_hash = SHA-256( "earnproof:issuer-metadata-uri:v1" || uri_utf8_bytes )
```

- The prefix is ASCII, with no trailing separator byte; the payload is appended
  directly.
- `canonical_document_bytes` is the metadata document serialized in the
  backend's canonical form (stable key ordering, no insignificant whitespace).
- `uri_utf8_bytes` is the canonical absolute URI, UTF-8 encoded, with no
  trailing whitespace or fragment.

The contract treats both results as opaque `BytesN<32>` and stores and echoes
them byte-for-byte — it never re-derives or re-hashes them. The golden-vector
tests in `contracts/issuer-registry/src/lib.rs` pin this encoding parity: the
bytes a backend supplies are exactly the bytes an indexer reads back from the
record and from the `issuer_registered` / `issuer_metadata_updated` events.

## Validation

- A commitment must be non-empty. The all-zero digest is rejected with
  `IssuerError::InvalidMetadataCommitment` when supplied to
  `set_issuer_metadata_commitment`, because it is reserved as the "no URI
  commitment recorded" sentinel and is not a value a real SHA-256 digest
  produces in practice.
- `register_issuer` records `metadata_uri_hash` as the all-zero sentinel and
  `metadata_revision = 1`. A real URI commitment is set afterwards with
  `set_issuer_metadata_commitment`, which requires both commitments to be
  non-empty and increments the revision.
