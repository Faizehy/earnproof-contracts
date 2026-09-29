# Contract events

What the EarnProof contracts publish, what they deliberately do not, and the
guarantees an indexer or reconciliation job may rely on.

Events are the only push-based view of contract state. Everything else requires
polling storage. That makes them load-bearing for indexers and for backend
reconciliation — and it makes a wrong event worse than a missing one, because a
missing event is eventually noticed while a wrong one is silently committed.

- Fixtures: [`tests/fixtures/events/`](../tests/fixtures/events/)
- Fixture validation: [`tests/event-fixtures/`](../tests/event-fixtures/)
- Behavioural tests: [`tests/events/`](../tests/events/)

## Guarantees

1. **Exactly once on success.** Every state-changing entry point that is
   documented as emitting publishes its event exactly once when it commits.
2. **Silence on failure.** A rejected invocation publishes nothing at all — not
   a failure event, and certainly not a success-shaped one.
3. **Payload matches committed state.** Every field equals what was written to
   storage in the same invocation.
4. **Deterministic ordering.** No entry point emits more than one event, so a
   sequence of N operations yields N events in invocation order.
5. **No protected data.** Payloads carry hashes, public addresses, version
   numbers, and timestamps. Never an amount, a memo, an identity, or a secret.

Each is asserted in [`tests/events/`](../tests/events/); the mapping is in
[Test coverage](#test-coverage) below.

## Event catalogue

### `protocol-config`

| Topic | Emitted by | Payload |
|---|---|---|
| `initialized` | `initialize` | `admin` |
| `admin_changed` | `set_admin` | `new_admin` |
| `paused` | `pause` | `paused` (always `true`) |
| `unpaused` | `unpause` | `paused` (always `false`) |
| `schema_approved` | `approve_schema_version` | `version` |
| `schema_deprecated` | `deprecate_schema_version` | `version` |

### `issuer-registry`

| Topic | Emitted by | Payload |
|---|---|---|
| `issuer_registered` | `register_issuer` | `issuer_id_hash`, `issuer_address`, `metadata_hash`, `metadata_uri_hash`, `metadata_revision`, `created_at` |
| `issuer_metadata_updated` | `update_issuer`, `set_issuer_metadata_commitment` | `issuer_id_hash`, `metadata_hash`, `metadata_uri_hash`, `metadata_revision`, `updated_at` |
| `issuer_suspended` | `suspend_issuer` | `issuer_id_hash`, `effective_ledger`, `effective_timestamp`, `updated_at` |
| `issuer_reactivated` | `reactivate_issuer` | `issuer_id_hash`, `effective_ledger`, `effective_timestamp`, `updated_at` |
| `issuer_revoked` | `revoke_issuer` | `issuer_id_hash`, `effective_ledger`, `effective_timestamp`, `updated_at` |
| `issuer_address_rotated` | `rotate_issuer_address` | `issuer_id_hash`, `old_address`, `new_address`, `updated_at` |

`issuer_address_rotated` carries both addresses so an indexer can update its
address→issuer mapping without scanning storage. An indexer that ignores
`old_address` will keep routing to a rotated-out key.

`issuer_registered` and `issuer_metadata_updated` carry **both** metadata
commitments. `metadata_hash` commits to the canonical metadata document
(content); `metadata_uri_hash` commits to the canonical document URI (location),
so an off-chain resolver can distinguish a change of location from a change of
content. At registration `metadata_uri_hash` is the all-zero "no URI commitment
recorded" sentinel until `set_issuer_metadata_commitment` sets it.
`metadata_revision` starts at `1` and increments on every accepted metadata
update. See [`metadata-commitment.md`](./metadata-commitment.md) for the
canonical-byte and domain-separation rules.

The status lifecycle events carry `effective_ledger` and `effective_timestamp`:
the ledger sequence and timestamp at which the suspension, reactivation, or
revocation became effective. Legacy records predating these fields carry the
documented `0` sentinel.

### `proof-registry`

| Topic | Emitted by | Payload |
|---|---|---|
| `proof_registered` | `register_proof` | `proof_id_hash`, `issuer_address`, `schema_version`, `created_ledger`, `created_at`, `expires_at` |

`proof_registered` carries the on-chain creation timing (`created_ledger` and
`created_at`), both sourced only from the host ledger environment, so an indexer
can record deterministic audit timestamps without a follow-up `get_proof`.
Revocation (`revoke_proof`, `admin_revoke_proof`) remains silent; proof state is
read with `get_proof`, `is_valid_proof`, `proof_validity`, and `is_revoked`.

### Silent entry points

Not every mutation emits. These do not, and the omission is deliberate:

| Contract | Entry point | Why it is silent |
|---|---|---|
| `issuer-registry` | `initialize` | Only `protocol-config` announces initialization. An indexer keying deployment off an event should watch that contract. |
| `proof-registry` | `initialize` | As above. |
| `proof-registry` | `revoke_proof`, `admin_revoke_proof` | Revocation stores state without announcing it; read it with `is_revoked` / `proof_validity`. |

## Topic naming

`#[contractevent]` derives the topic from the struct name in snake_case. The
struct `IssuerAddressRotated` publishes under the topic
`issuer_address_rotated`. Indexers must match on the topic form.

Every event carries **exactly one topic**: the discriminant. There are no
additional indexed topics, so an indexer cannot filter server-side by issuer or
by version — that filtering happens after decoding the payload. Adding a second
topic would be a breaking change for anyone filtering on topic arity, and
`event_topics_are_single_symbol_discriminants` fails if one appears.

## Failure semantics

Every contract in this workspace signals rejection by panicking, and a panicking
invocation is rolled back. Storage and events roll back together, so a failed
call is indistinguishable from one never attempted.

This matters most for **cross-contract** calls. `proof-registry::register_proof`
reads the pause flag from `protocol-config` and issuer status from
`issuer-registry` before committing. If either check rejects, nothing is
published by *any* of the three contracts — there is no partial sequence in
which a callee announces something the caller then discards.

The failure classes covered by tests:

| Class | Example |
|---|---|
| Authorization | Re-initializing an initialized contract |
| Duplicate | Registering an existing issuer id, address, or proof id |
| Paused | Registering a proof while the protocol is paused |
| Invalid schema | Unapproved version, deprecated version, version zero |
| Revoked issuer | Registration by a revoked or suspended issuer; reactivating a revoked issuer |
| Expiry | Proof expiry in the past, or equal to the current ledger time |
| Missing record | Suspending an unknown issuer, revoking an unknown proof |
| Cross-contract | Any of the above reached through `register_proof` |

## Ordering

Ordering is fixed by the ledger, not by the contracts. Because no entry point
emits more than one event, the guarantee reduces to: **events appear in the order
their invocations were applied.**

A multi-step operator sequence therefore reads exactly as issued. For the
incident-response sequence — pause, suspend, revoke, unpause — an indexer sees:

```
paused
issuer_suspended
issuer_revoked
unpaused
```

A rejected step contributes nothing and leaves no gap, so a consumer cannot
infer a state transition that did not happen.

## Privacy

The contracts store only hashes. `issuer_id_hash` and `metadata_hash` are 32-byte
digests computed by the backend; the contracts never see the values behind them
and cannot leak what they do not hold.

What events may carry:

- **Hashes** — opaque 32-byte digests.
- **Public Stellar addresses** — public by construction on a public ledger.
- **Version numbers and timestamps** — not subject-linked.

What events must never carry: amounts, balances, income figures, payment
history, memos, names, email addresses, identity documents, wallet secrets,
signatures, or key material.

`no_event_payload_carries_protected_data` enforces this by scanning payload
field names against a forbidden list. It is a name-shape check, not proof of
absence — but it fails loudly on the realistic mistake, which is a field named
`amount` or `memo` being added to an event without anyone noticing what it
implies.

The stakes justify the caution: an event is published to every indexer on the
network, is retained by parties outside this project's control, and **cannot be
recalled**. A leak here is permanent in a way a database leak is not.

## Compatibility

Fixtures under [`tests/fixtures/events/`](../tests/fixtures/events/) are the
published contract. Each declares its topics, payload fields, and a
`compatibility` classification:

- **stable** — no structural change.
- **additive** — new optional payload fields; existing fields and topics
  unchanged. Indexers ignoring unknown fields stay compatible.
- **breaking** — a field removed, renamed, or retyped. Indexers must update.

`tests/events/src/compatibility.rs` compares live emissions against these
declarations. A renamed topic, a dropped field, or an extra one fails there —
which is what makes the fixtures a contract rather than documentation that
happened to be true once.

### Changing an event

1. Change the contract.
2. Update the fixture: payload fields, `schema_version`, `compatibility`.
3. Update `DECLARED_EVENTS` in `tests/events/src/compatibility.rs`.
4. Update the catalogue above.
5. For a breaking change, create a new version directory (`v2/`) so the previous
   shape stays readable.

Steps 1–3 are enforced: skipping any one fails the suite.

## Test coverage

`tests/events/` (50 tests):

| Module | Covers | Guarantee |
|---|---|---|
| `emission.rs` | Each documented event fires once, with a payload matching committed storage | 1, 3 |
| `ghost.rs` | Twenty-one failure cases across every documented class publish nothing | 2 |
| `ordering.rs` | Arity, multi-step sequences, gap-free failures, payload privacy, emitter attribution | 4, 5 |
| `compatibility.rs` | Live emissions match fixture declarations in both directions | — |

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo build --workspace
```

### A note on the test environment

`env.events().all()` returns the events of the **most recent invocation**, not
an accumulated log. Events must therefore be read immediately after the call
that produced them; `Deployment::capture` in
[`tests/events/src/harness.rs`](../tests/events/src/harness.rs) exists for that
reason. A test that performs several operations and inspects the stream
afterwards will see only the last one — and would pass vacuously while appearing
to check a sequence.
