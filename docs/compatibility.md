# Contract compatibility policy

How a change to a contract artifact is classified, what a release must record,
and what a breaking change requires before it ships.

Consumers — the EarnProof backend, indexers, and any third party reading the
registries — need to know when an artifact changes behaviour they depend on.
Without a stated policy, "we changed a contract" and "we broke your integration"
are indistinguishable until something fails in production.

- Release notes: [`docs/releases/`](releases/)
- Event fixtures: [`tests/fixtures/events/`](../tests/fixtures/events/)
- Deployment manifests: [`scripts/`](../scripts/)

## Change classes

Every change to a contract crate falls into exactly one class. The class
determines what the release must carry and who has to approve it.

### 1. ABI

The set of public entry points and their signatures.

| Change | Class |
|---|---|
| Adding a new entry point | **Additive** |
| Adding a parameter to an existing entry point | **Breaking** |
| Removing or renaming an entry point | **Breaking** |
| Changing a parameter or return type | **Breaking** |
| Reordering parameters | **Breaking** |

Soroban dispatches by function name and positional arguments. A caller built
against the old signature does not fail to compile — it fails at invocation, in
production.

### 2. Storage

Keys in `DataKey` and the types they hold.

| Change | Class |
|---|---|
| Adding a new key variant | **Additive** |
| Adding a field to a stored struct | **Breaking** |
| Adding a separate optional sidecar key without changing the stored struct | **Additive** |
| Removing or renaming a key variant | **Breaking** |
| Changing a stored type | **Breaking** |
| Changing a TTL constant | **Semantic** |

Adding a field to a stored struct is breaking, not additive: existing entries
were serialised without it and will fail to decode. There is no migration
mechanism in these contracts, so a storage change means redeployment and
off-chain trust migration. See
[#12](https://github.com/veridatum-labs/earnproof-contracts/issues/12).

### 3. Events

Topics and payload fields. The published shapes live in
[`tests/fixtures/events/`](../tests/fixtures/events/), whose `compatibility`
field classifies each change.

| Change | Class |
|---|---|
| Adding a new event | **Additive** |
| Adding a payload field | **Additive** — indexers ignoring unknown fields stay compatible |
| Removing or renaming a topic or field | **Breaking** |
| Changing a payload field type | **Breaking** |
| Adding a second indexed topic | **Breaking** — breaks topic-arity filters |

### 4. Errors

Panic messages and the conditions that trigger them.

| Change | Class |
|---|---|
| Adding a new rejection condition | **Semantic** — previously accepted calls now fail |
| Removing a rejection condition | **Semantic** — previously rejected calls now succeed |
| Changing a panic message | **Additive** |

Changing a message is only additive because panic strings are not a stable
interface: a cross-contract rejection surfaces as `Error(WasmVm, InvalidAction)`
and the message reaches the caller only through the diagnostic log. Consumers
must not match on them. Typed errors are
[#10](https://github.com/veridatum-labs/earnproof-contracts/issues/10).

### 5. Authorization

Who may call an entry point.

| Change | Class |
|---|---|
| Tightening a requirement | **Breaking** — a caller authorised yesterday is not today |
| Relaxing a requirement | **Semantic** — and a security decision |
| Changing which address is checked | **Breaking** |

Every authorization change is a security change. Relaxing one is never routine,
even though it breaks no caller.

### 6. Resource

CPU instructions, memory, and ledger footprint.

| Change | Class |
|---|---|
| Any measurable increase | **Semantic** |
| An increase that pushes an operation past network limits | **Breaking** |

A contract that exceeds a resource limit is unusable regardless of its ABI.
There are no budget regression gates yet —
[#19](https://github.com/veridatum-labs/earnproof-contracts/issues/19) — so
resource impact is currently assessed by hand and stated in the release note.

### 7. Semantic

Behaviour changes with no interface change: the same call, a different outcome.

Always requires an explicit note. This class is the most dangerous precisely
because nothing in the type system or the ABI surfaces it. A consumer's tests
keep passing while the meaning underneath has moved.

## Classification summary

| Class | Consumer action | Governance |
|---|---|---|
| **Additive** | None; deploy at will | Normal review |
| **Semantic** | Read the release note; retest affected flows | Normal review + explicit note |
| **Breaking** | Update integration before the new artifact is used | Explicit governance (below) |

When a change spans several classes, the strictest applies.

## Release requirements

Every release note in [`docs/releases/`](releases/) records:

| Field | Why |
|---|---|
| `release` | Identifier, e.g. `v0.1.0` |
| `date` | ISO-8601 date |
| `commit` | Full 40-character source commit |
| `toolchain` | Rust channel and `soroban-sdk` version |
| `contracts[].version` | Crate version per contract |
| `contracts[].wasm_sha256` | SHA-256 of the built artifact |
| `changes[]` | Each change with its class and rationale |
| `migration` | Required steps, or an explicit "none" |
| `backend_compatibility` | Minimum backend version, or "unchanged" |
| `rollback` | How to revert, or why it is not possible |

An artifact whose declared metadata does not match its manifest is rejected by
`scripts/verify-manifest.ps1 -Release`. Recording a hash is not enough; the
recorded hash has to be the deployed one.

### What a release note must never contain

- Admin secret keys or seed phrases
- Signing material of any kind
- Private RPC endpoints, API keys, or credentials
- Internal infrastructure hostnames or addresses
- Deployer account secrets

Contract IDs, public Stellar addresses, WASM hashes, and transaction hashes are
public ledger data and belong in the note. The distinction is not "sensitive
versus not" but "already public versus not" — a release note is published, and
publication is irreversible.

`scripts/verify-manifest.ps1 -Release` scans for credential-shaped material and
fails on a match, so the rule is enforced rather than merely stated.

## Breaking-change governance

A breaking change requires all four of the following **before** the artifact is
deployed anywhere consumers can reach it:

1. **Approval** from a maintainer listed in [`MAINTAINERS.md`](../MAINTAINERS.md),
   recorded in the release note by name.
2. **A migration plan** — the exact steps a consumer takes, or an explicit
   statement that none is possible and what that costs them.
3. **A rollback plan** — how to return to the previous artifact, or why it is
   irreversible. "Redeploy the old WASM" is only a rollback if state is
   compatible; if it is not, say so.
4. **Containment notes** — what an operator does if the change misbehaves in
   production. Usually `pause()` on `protocol-config`, but a change to the pause
   path itself needs a different answer, and that answer belongs here.

A release note claiming a breaking change without all four is incomplete, and
`verify-manifest.ps1 -Release` fails on it.

### Why rollback is hard here

All three contracts have an in-place upgrade mechanism
(`approve_upgrade`/`upgrade_contract`, documented in
[`docs/contract-upgrades.md`](contract-upgrades.md)) — the WASM behind an
existing contract ID can be swapped without redeploying. That mechanism
swaps **code**, not **storage**: there is no migration hook, so it only
carries a change through cleanly when the new code's deserialization
tolerates the old stored bytes.

For a storage-incompatible change, this means rollback still requires
deploying the previous WASM to a **new** contract ID and re-pointing every
consumer, because the old ID would now be running code that can't safely
read state written under the newer shape. State does not travel with it.
See [`docs/contract-upgrades.md`](contract-upgrades.md#storage-migration-strategy)
for the two concrete strategies for this case.

That is the real cost of a breaking storage change in this repository, and
it is why the governance bar is set where it is — an ABI-only or
events-only breaking change can go through the in-place upgrade path and
still needs governance sign-off, but does not carry the same
new-contract-ID cost.

## Versioning

Contract crate versions follow semver, interpreted against the classes above:

- **Patch** (`0.1.0` → `0.1.1`) — additive changes only.
- **Minor** (`0.1.0` → `0.2.0`) — semantic changes.
- **Major** (`0.1.0` → `1.0.0`) — any breaking change.

Contracts version independently. A release may bump one and leave the others
untouched; the release note lists each contract with its own version and hash.

## Backend compatibility

The backend depends on contract behaviour in three ways, and each has a distinct
failure mode:

1. **Invocation** — entry-point names and signatures. An ABI change breaks
   anchoring at the point of submission.
2. **Hashing** — it computes `proof_id_hash`, `commitment_hash`, and
   `metadata_hash`. The contracts treat these as opaque and never verify their
   construction, so a hashing change is invisible on-chain until proofs stop
   matching. Vectors are unpublished —
   [#43](https://github.com/veridatum-labs/earnproof-contracts/issues/43).
3. **Schema versions** — it submits a version the protocol must have approved.

Every release states a minimum backend version or "unchanged". A release that
changes any of the three and claims "unchanged" is wrong, and it is the kind of
wrong that surfaces as a production outage rather than a failed build.

## Maintenance

- Adding a change class: update the tables above and the validation in
  `verify-manifest.ps1`.
- Cutting a release: copy [`docs/releases/TEMPLATE.md`](releases/TEMPLATE.md),
  fill every field, then run
  `pwsh -File scripts/verify-manifest.ps1 -Manifest <manifest> -Release <note>`.
- Changing the required fields: update the template, the validation, and
  `scripts/verify-manifest.tests.ps1` together — the tests assert the field set.
