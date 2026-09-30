# Security evidence index

A navigable map from threat assumptions and invariants to the exact code, tests,
and artifacts that back them. It is written for an independent reviewer arriving
without prior context on this repository.

**Commit:** `09f9841c9af78e67c90f0eaab1039052b17b9a03`
**Branch:** `develop`
**Toolchain:** `rust-toolchain.toml` — channel `stable`, components `rustfmt`, `clippy`
**SDK:** `soroban-sdk 27.0.0` (`Cargo.toml`)

Every path below is repository-relative and every line number is valid at the
commit named above. If the commit differs from the one you are reviewing, treat
this index as stale and see [Refresh checklist](#refresh-checklist).

**Related documents:**

- [`docs/threat-model.md`](../threat-model.md) — the structured threat model
  this index draws its threat assumptions from: trust boundaries, actors,
  attack surface, and mainnet release gates. Read that document first for
  narrative context; this index is the code-and-test-level evidence trail for
  the claims it makes.
- [`SECURITY.md`](../../SECURITY.md) — how to report a vulnerability
  (`security@veridatum.dev`) and what to include in a report.

## How to read this index

Each claim carries an explicit status. The distinction matters: an auditor
should never have to guess whether "the admin is checked" means the code does it,
the tests prove it, or an operator is trusted to do it.

| Status | Meaning |
|---|---|
| **Implemented** | The control exists in contract code. Path and line given. |
| **Tested** | The control has a test that fails if it regresses. Path and test name given. |
| **Assumption** | Correctness depends on something outside these contracts — an operator procedure, the backend, or the network. Not enforced on-chain. |
| **Accepted risk** | A known weakness the project has decided not to mitigate, with the reasoning recorded. |
| **Open gap** | Known missing coverage. Links to a tracking issue. |

A control marked *Implemented* but not *Tested* is a review priority: nothing
prevents it from regressing silently.

## Contents

- [Assets](#assets)
- [Entry points and privileges](#entry-points-and-privileges)
- [Invariants](#invariants)
- [Errors](#errors)
- [Events](#events)
- [Storage and TTL](#storage-and-ttl)
- [Cross-contract calls](#cross-contract-calls)
- [Artifact provenance](#artifact-provenance)
- [Known limitations and out-of-scope controls](#known-limitations-and-out-of-scope-controls)
- [Open gaps](#open-gaps)
- [Verification commands](#verification-commands)
- [Refresh checklist](#refresh-checklist)

## Assets

What an attacker would want to reach. None of these is a fund; the contracts
custody no value. The asset is *trust state* — what relying parties believe
about an issuer or a proof.

| Asset | Where it lives | Compromise impact | Status |
|---|---|---|---|
| Protocol administrator | `contracts/protocol-config/src/lib.rs:11` (`DataKey::Admin`) | Full control of pause state and schema approval | Implemented; auth untested |
| Pause flag | `contracts/protocol-config/src/lib.rs:12` (`DataKey::Paused`) | Containment defeated, or protocol frozen | Implemented, Tested |
| Schema approval set | `contracts/protocol-config/src/lib.rs:14` (`DataKey::SchemaVersion(u32)`) | Proofs admitted under an unreviewed schema | Implemented, Tested |
| Issuer registry admin | `contracts/issuer-registry/src/lib.rs:10` (`DataKey::Admin`) | Attacker registers themselves as a trusted issuer | Implemented; auth untested |
| Issuer records and status | `contracts/issuer-registry/src/lib.rs:10` (`DataKey::Issuer`) | Revoked issuer appears active; active issuer appears revoked | Implemented, Tested |
| Address → issuer mapping | `contracts/issuer-registry/src/lib.rs:10` (`DataKey::AddressIssuer`) | Compromised key keeps issuing under a valid identity | Implemented, Tested |
| Proof registry admin | `contracts/proof-registry/src/lib.rs:22` (`DataKey::Admin`) | Arbitrary revocation of valid proofs | Implemented; auth untested |
| Proof records and revocation state | `contracts/proof-registry/src/lib.rs:21` (`DataKey::Proof`) | Revoked credential still verifies, or valid one does not | Implemented, Tested |

**No asset in this list is private data.** The contracts store only hashes
(`proof_id_hash`, `commitment_hash`, `metadata_hash`, `issuer_id_hash`) and
Stellar addresses. Income figures, payment history, memos, and raw credentials
never reach the chain. See [Known limitations](#known-limitations-and-out-of-scope-controls)
for what this does *not* guarantee.

## Entry points and privileges

Every public function, who may call it, and where that requirement is enforced.

### `contracts/protocol-config/src/lib.rs`

| Entry point | Line | Caller | Auth enforced at | Status |
|---|---|---|---|---|
| `initialize` | 49 | Anyone, once | `:54` `require_auth(&admin)` + `:50` re-init guard | Implemented; auth untested |
| `get_admin` | 64 | Anyone | — (read) | Implemented, Tested |
| `set_admin` | 71 | Current admin | `:73` | Implemented; auth untested |
| `is_paused` | 79 | Anyone | — (read) | Implemented, Tested |
| `pause` | 86 | Current admin | `:88` | Implemented; auth untested |
| `unpause` | 94 | Current admin | `:96` | Implemented; auth untested |
| `approve_schema_version` | 102 | Current admin | `:104` | Implemented; auth untested |
| `deprecate_schema_version` | 114 | Current admin | `:116` | Implemented; auth untested |
| `is_schema_version_approved` | 126 | Anyone | — (read) | Implemented, Tested |
| `get_config_version` | 143 | Anyone | — (read) | Implemented, Tested |

### `contracts/issuer-registry/src/lib.rs`

| Entry point | Line | Caller | Auth enforced at | Status |
|---|---|---|---|---|
| `initialize` | 75 | Anyone, once | `:80` + `:76` re-init guard | Implemented; auth untested |
| `get_admin` | 85 | Anyone | — (read) | Implemented, Tested |
| `register_issuer` | 92 | Admin | `:94` | Implemented; auth untested |
| `update_issuer` | 137 | Admin | `:139` | Implemented; auth untested |
| `suspend_issuer` | 166 | Admin | via `set_status` `:256` | Implemented; auth untested |
| `reactivate_issuer` | 170 | Admin | via `set_status` `:256` | Implemented; auth untested |
| `revoke_issuer` | 174 | Admin | via `set_status` `:256` | Implemented; auth untested |
| `rotate_issuer_address` | 178 | Admin | `:180` | Implemented; auth untested |
| `get_issuer` | 221 | Anyone | — (read) | Implemented, Tested |
| `is_active_issuer` | 232 | Anyone | — (read) | Implemented, Tested |
| `is_active_address` | 237 | Anyone | — (read) | Implemented, Tested |
| `get_issuer_by_address` | 315 | Anyone | — (read) | Implemented, Tested |

### `contracts/proof-registry/src/lib.rs`

| Entry point | Line | Caller | Auth enforced at | Status |
|---|---|---|---|---|
| `initialize` | 30 | Anyone, once | `:41` + `:36` re-init guard | Implemented; auth untested |
| `register_proof` | 51 | The named issuer | `:59` `require_auth(&issuer_address)` | Implemented; auth untested |
| `revoke_proof` | 106 | The proof's issuer | via `set_revoked` `:166` | Implemented; auth untested |
| `admin_revoke_proof` | 110 | Registry admin | via `set_revoked` `:162` | Implemented; auth untested |
| `get_proof` | 114 | Anyone | — (read) | Implemented, Tested |
| `is_valid_proof` | 125 | Anyone | — (read) | Implemented, Tested |
| `is_revoked` | 130 | Anyone | — (read) | Implemented, Tested |
| `get_admin` | 135 | Anyone | — (read) | Implemented, Tested |
| `get_issuer_registry` | 142 | Anyone | — (read) | Implemented, Tested |
| `get_protocol_config` | 149 | Anyone | — (read) | Implemented, Tested |

**Reviewer note — authorization is largely unproven.** Every test in this
repository calls `env.mock_all_auths()`, which admits every call regardless of
who signed. A test that invokes a privileged function and observes success
therefore proves that the function *works*, not that it is *guarded*. Removing
a `require_auth` line would not fail a single existing test.

The `Auth enforced at` column above is the result of reading the source, not of
executing a test. Treat it as a claim to verify, not as evidence. Two techniques
would turn it into evidence — inspecting the recorded auth tree
(`env.auths()`) to assert which address the contract actually demanded, or
`mock_auths` with a specific non-admin signer to observe rejection — and neither
is currently used. Tracked as
[#34](https://github.com/veridatum-labs/earnproof-contracts/issues/34)
(negative matrix) and
[#38](https://github.com/veridatum-labs/earnproof-contracts/issues/38)
(mutation testing).

Accordingly, the status column in the three tables above reads **Implemented**
for authorization on every privileged entry point, and **Tested** only for the
functional behaviour of that entry point.

## Invariants

Properties that must hold across every reachable state.

| # | Invariant | Enforced at | Proven by | Status |
|---|---|---|---|---|
| I1 | A contract can be initialized at most once | `protocol-config:50`, `issuer-registry:76`, `proof-registry:36` | — | Implemented, **untested** ([#34](https://github.com/veridatum-labs/earnproof-contracts/issues/34)) |
| I2 | Only the current admin can change pause state | `protocol-config:88`, `:96` | — | Implemented, **untested** ([#34](https://github.com/veridatum-labs/earnproof-contracts/issues/34)) |
| I3 | A rotated-out admin retains no authority | `protocol-config:72` (admin read at call time, never cached) | — | Implemented, **untested** ([#44](https://github.com/veridatum-labs/earnproof-contracts/issues/44)) |
| I4 | Admin rotation does not change pause state | `protocol-config:71–77` (no pause write) | — | Implemented, **untested** ([#44](https://github.com/veridatum-labs/earnproof-contracts/issues/44)) |
| I5 | Each contract's admin is independent | Separate `DataKey::Admin` per contract | — | Implemented, **untested** ([#11](https://github.com/veridatum-labs/earnproof-contracts/issues/11)) |
| I6 | `config_version` is monotonic across privileged mutations | `protocol-config:157` `bump_config_version` | `protocol-config` `pause_and_unpause_bump_config_version` (`:214`) | Implemented, Tested (pause/unpause only) |
| I7 | Pause contains `register_proof` | `proof-registry:71–73` | `proof-registry` `rejects_registration_when_protocol_is_paused` (`:324`) | Implemented, Tested |
| I8 | Pause contains *only* `register_proof`; containment operations stay available | Registry contracts do not read the pause flag | — | Implemented, **untested** ([#44](https://github.com/veridatum-labs/earnproof-contracts/issues/44)) |
| I9 | Issuer revocation is terminal | `issuer-registry:258–260` | `issuer-registry` `revoked_issuer_cannot_be_reactivated` (`:417`), `status_transitions_reject_reactivated_revoked_issuer` (`:384`) | Implemented, Tested |
| I10 | A revoked or suspended issuer cannot register proofs | `proof-registry:80–84` | `proof-registry` `rejects_inactive_issuer_address` (`:339`) | Implemented, Tested |
| I11 | A proof can be revoked at most once | `proof-registry:171–173` | — | Implemented, **untested** ([#34](https://github.com/veridatum-labs/earnproof-contracts/issues/34)) |
| I12 | A rejected call leaves no partial state | Soroban invocation atomicity | Partially, via the `*_emits_no_event` tests in `issuer-registry` (`:484`, `:524`, `:615`) | Assumption (host-provided), partially Tested ([#36](https://github.com/veridatum-labs/earnproof-contracts/issues/36)) |
| I13 | Proof registration requires an approved schema | `proof-registry:75–77` | `proof-registry` `rejects_unapproved_schema_version` (`:310`) | Implemented, Tested |
| I14 | Proof expiry must be in the future at registration | `proof-registry:65–67` | `proof-registry` `rejects_expired_proof` (`:285`) | Implemented, Tested |
| I15 | A rotated-out issuer address stops resolving | `issuer-registry:200–202` | `issuer-registry` `rotate_address_emits_one_event` (`:597`) covers the event, not the mapping release | Implemented, **partially tested** ([#34](https://github.com/veridatum-labs/earnproof-contracts/issues/34)) |
| I16 | One issuer id, and one address, maps to at most one issuer | `issuer-registry:103–105`, `:107–109`, `:194–196` | `issuer-registry` `rejects_duplicate_issuer_id` (`:402`) | Implemented, Tested |
| I17 | Schema version 0 is never approvable | `protocol-config:151–153`, `:127–129` | `protocol-config` `rejects_zero_schema_version` (`:239`) | Implemented, Tested |
| I18 | A proof id cannot be registered twice | `proof-registry:86–88` | `proof-registry` `rejects_duplicate_proof_id` (`:299`) | Implemented, Tested |
| I19 | Each state-mutating issuer call emits exactly one event, and a rejected one emits none | `issuer-registry` `set_status` (`:264–284`) and per-function publishes | `issuer-registry` `each_mutation_emits_exactly_one_event` (`:639`) and the four `*_emits_no_event` tests | Implemented, Tested |
| I20 | Operation order does not change the final state beyond the documented rules | — | — | **Untested** ([#18](https://github.com/veridatum-labs/earnproof-contracts/issues/18), [#44](https://github.com/veridatum-labs/earnproof-contracts/issues/44)) |

**Reviewer priority.** Seven invariants above are implemented but unproven —
I1, I2, I3, I4, I5, I11, and I20. Nothing in CI fails if any of them regresses.
I2 and I3 in particular are the controls standing between a rotated-out
administrator and the pause switch. These should be the first thing an external
reviewer examines by hand.

## Errors

All failures are Rust `panic!` with fixed string literals. No error interpolates
a call argument, so no error can leak an identifier or a payload.

| Contract | Line | Message |
|---|---|---|
| `protocol-config` | 51 | `already initialized` |
| `protocol-config` | 152 | `schema version must be greater than zero` |
| `protocol-config` | 66, 145 | `not initialized` (via `expect`) |
| `issuer-registry` | 77 | `already initialized` |
| `issuer-registry` | 103 | `issuer already registered` |
| `issuer-registry` | 108, 195 | `issuer address already registered` |
| `issuer-registry` | 149 | `revoked issuer cannot be updated` |
| `issuer-registry` | 190 | `revoked issuer cannot rotate address` |
| `issuer-registry` | 259 | `revoked issuer cannot be reactivated` |
| `proof-registry` | 37 | `already initialized` |
| `proof-registry` | 62 | `schema version must be greater than zero` |
| `proof-registry` | 66 | `proof expiration must be in the future` |
| `proof-registry` | 72 | `protocol is paused` |
| `proof-registry` | 76 | `schema version is not approved` |
| `proof-registry` | 82 | `issuer is not active` |
| `proof-registry` | 87 | `proof already registered` |
| `proof-registry` | 172 | `proof already revoked` |

**Status: Implemented; privacy property Tested; stability an [open gap](#open-gaps).**

Two consequences a reviewer should weigh:

1. Panic strings are not a stable ABI. A caller cannot match on them across
   versions, and a cross-contract rejection surfaces to the outer caller as
   `Error(WasmVm, InvalidAction)` — the specific reason appears only in the
   diagnostic event log. Callers must not attempt to distinguish failure causes
   from the returned error value. Tracked as
   [#10](https://github.com/veridatum-labs/earnproof-contracts/issues/10).
2. `expect("...")` on a missing storage read is indistinguishable from a
   deliberate rejection at the call boundary. An operator debugging a failure
   cannot tell "not initialized" from "key expired" without the diagnostic log.

## Events

| Contract | Event | Line | Fields |
|---|---|---|---|
| `protocol-config` | `Initialized` | 17 | `admin` |
| `protocol-config` | `AdminChanged` | 22 | `new_admin` |
| `protocol-config` | `Paused` | 27 | `paused` |
| `protocol-config` | `Unpaused` | 32 | `paused` |
| `protocol-config` | `SchemaApproved` | 37 | `version` |
| `protocol-config` | `SchemaDeprecated` | 42 | `version` |
| `issuer-registry` | `IssuerRegistered` | — | `issuer_id_hash`, `issuer_address`, `metadata_hash`, `created_at` |
| `issuer-registry` | `IssuerMetadataUpdated` | — | `issuer_id_hash`, `metadata_hash`, `updated_at` |
| `issuer-registry` | `IssuerSuspended` | — | `issuer_id_hash`, `updated_at` |
| `issuer-registry` | `IssuerReactivated` | — | `issuer_id_hash`, `updated_at` |
| `issuer-registry` | `IssuerRevoked` | — | `issuer_id_hash`, `updated_at` |
| `issuer-registry` | `IssuerAddressRotated` | — | `issuer_id_hash`, `old_address`, `new_address`, `updated_at` |
| `proof-registry` | **none** | — | — |

Versioned fixtures: `tests/fixtures/events/`, schema at
`tests/fixtures/events/schema.json`, validated by `tests/event-fixtures/`
(13 tests).

**`proof-registry` emits no events.** Proof registration and revocation are
observable only by reading storage, so an off-chain indexer cannot build a
revocation timeline from the event stream. This is an
[open gap](#open-gaps), tracked as
[#3](https://github.com/veridatum-labs/earnproof-contracts/issues/3).

Every event field is a hash, an address, a version number, or a timestamp. No
event carries a private value. **Status: Implemented, Tested.**

## Storage and TTL

Full model: `docs/storage-model.md`. Constants: `packages/shared/src/lib.rs:5–6`.

| Constant | Value | Approx. wall clock |
|---|---|---|
| `TTL_THRESHOLD_LEDGERS` | 50,000 | ~3 days |
| `TTL_EXTEND_TO_LEDGERS` | 500,000 | ~29 days |

| Contract | Key | Class | Extended by |
|---|---|---|---|
| `protocol-config` | `Admin`, `Paused`, `ConfigVersion` | Instance | every mutating call, via `bump_config_version` → `extend_instance_ttl` (`:157`, `:163`) |
| `protocol-config` | `SchemaVersion(u32)` | Persistent | `approve`/`deprecate` (`:169`), and `is_schema_version_approved` on read (`:133`) |
| `issuer-registry` | `Admin` | Instance | `initialize` only (`:81`) |
| `issuer-registry` | `Issuer(BytesN<32>)`, `AddressIssuer(Address)` | Persistent | every read and write touching the key |
| `proof-registry` | `Admin`, `IssuerRegistry`, `ProtocolConfig` | Instance | `initialize` only (`:49`) |
| `proof-registry` | `Proof(BytesN<32>)` | Persistent | every read and write touching the key |

**Accepted risk — instance TTL on the registries.** Neither registry extends its
instance TTL after `initialize`. `protocol-config` extends on every mutation, but
the two registries only ever extend *persistent* keys. A registry that goes ~29
days without a state-mutating call can have its instance entry archived while
individual issuer and proof records survive. Recovery requires an on-chain
restore. The project accepts this for a testnet deployment with regular
activity; it is a **material finding for any mainnet review** and should be
re-evaluated before mainnet. Boundary tests are an
[open gap](#open-gaps) — [#35](https://github.com/veridatum-labs/earnproof-contracts/issues/35).

**Assumption — archival is not deletion.** Soroban archives expired entries
rather than erasing them. A reviewer assessing data-lifetime guarantees should
treat all on-chain state as permanent. Because the contracts store only hashes,
this is not a privacy exposure, but it does mean revocation state cannot be
"forgotten".

## Cross-contract calls

One call graph edge exists, and it is the protocol's single trust dependency.

| Caller | Callee | Interface | Line |
|---|---|---|---|
| `proof-registry` | `protocol-config` | `is_paused`, `is_schema_version_approved` | `contracts/proof-registry/src/lib.rs:6–10`, invoked `:69–77` |
| `proof-registry` | `issuer-registry` | `is_active_address` | `contracts/proof-registry/src/lib.rs:12–15`, invoked `:79–84` |

The callee addresses are fixed at `initialize` (`:44–48`) and there is **no
setter**. Rewiring a deployment requires redeploying `proof-registry`.

**Assumption.** `proof-registry` trusts whatever contract sits at the stored
addresses. If a wrong address were supplied at `initialize`, an attacker-controlled
contract could report `is_paused() == false` and `is_active_address() == true`
unconditionally. Nothing on-chain detects this; the control is the deployment
procedure and manifest verification (`scripts/verify-manifest.ps1`).

Disagreement between the two callees resolves toward containment: registration
requires *both* to allow it — `proof-registry:69-84` checks the pause flag and
the issuer status in sequence, and either alone is sufficient to reject. Each
condition is tested in isolation (`rejects_registration_when_protocol_is_paused`
at `:324`, `rejects_inactive_issuer_address` at `:339`); their combination is
not. **Implemented, partially Tested.**

Failure atomicity across the call boundary is an
[open gap](#open-gaps) — [#36](https://github.com/veridatum-labs/earnproof-contracts/issues/36).

## Artifact provenance

Testnet deployment record: `scripts/deployment-manifest.testnet.json`.
Placeholder template: `scripts/deployment-manifest.example.json`.
Verifier: `scripts/verify-manifest.ps1` (tests in `scripts/verify-manifest.tests.ps1`).

| Contract | Contract ID | WASM SHA-256 |
|---|---|---|
| `protocol-config` | `CC3OREX5QBIKJ5JOW36JFJJW7TLAKJOVT5WJXEITGALO7MU32KHICS2A` | `dd0a2d58bc634f09f94f92b09811714a25f36f4e0bd34c10dbac33238c84d594` |
| `issuer-registry` | `CB73TVWVJIIVNTKLWSHZB5NL2UIF3B3EUL4YH4MUD6EYX6SFIHE77D2F` | `bc57dd87f98da28779870e2494655fa6cbe8f8d5d6d5fbd4eb4f6a0a59070ec5` |
| `proof-registry` | `CCMTAXBWN2ZGEDVKGHT6GQENZSTBSLQAGYGGKJWNMDSTVRT2QNMMNWRK` | `4e2a433a775d7fcb1e002cea53a2a0c36c127d3026723da6b7d01491d4a0340c` |

Network: `stellar-testnet`. Deployed `2026-08-11T04:21:00Z`. Build target
`wasm32v1-none`, built with `stellar contract build`. Upload and invoke
transaction hashes are recorded in the manifest under `transactions`.

The manifest contains no credentials: only public contract IDs, public Stellar
addresses, content hashes, and transaction hashes. The deployer identity appears
as a named CLI profile (`earnproof-deployer`), never as a key.

**Open gap — the hashes above are not independently reproducible.** There is no
pinned `stellar` CLI version and no documented procedure to rebuild the WASM and
confirm the SHA-256 matches. A reviewer cannot currently verify that the deployed
bytecode corresponds to this source tree. Tracked as
[#17](https://github.com/veridatum-labs/earnproof-contracts/issues/17). Treat the
deployed contracts as unverified against source until it closes.

## Known limitations and out-of-scope controls

Stated so a reviewer does not spend time looking for controls that were never
intended to be here.

**Out of scope — backend.** These contracts do not implement authentication,
rate limiting, key custody, webhook delivery, input validation of off-chain
data, or any privacy control over data that never reaches the chain. All of that
lives in `earnproof-backend` and must be reviewed separately. A clean review of
this repository says nothing about the security of the system as a whole.

**Out of scope — network.** Consensus, transaction ordering, fee markets, RPC
availability, and the correctness of the Soroban host are assumed sound. Entry
sequencing, replay protection at the transaction level, and ledger timestamp
accuracy are network properties.

**Out of scope — hashing.** The contracts accept `proof_id_hash`,
`commitment_hash`, and `metadata_hash` as opaque 32-byte values and never verify
that they were derived correctly. Collision resistance and correct construction
are entirely the backend's responsibility. The hashing scheme is not yet
published — [#43](https://github.com/veridatum-labs/earnproof-contracts/issues/43).

**Accepted risk — admin stranding.** `set_admin` accepts any address without
verifying the successor can authorise. An operator can permanently strand a
paused contract by rotating to an address they do not control. The contracts do
not prevent this. The mitigation is observability, not prevention: every rotation
advances `config_version` and emits `AdminChanged`, so a monitor watching either
signal sees the change. Coordinated rotation is
[#11](https://github.com/veridatum-labs/earnproof-contracts/issues/11); a
documented emergency procedure is
[#44](https://github.com/veridatum-labs/earnproof-contracts/issues/44).

**Accepted risk — single admin key.** Each contract has exactly one
administrator address, with no multisig, timelock, or two-step handover.
Compromise of one key is immediate and total for that contract. Mitigated
operationally by the Stellar account itself being multisig-capable, which is an
assumption about deployment, not a contract control.

**Accepted risk — no upgrade path.** Contracts have no upgrade mechanism. A bug
requires redeployment and off-chain migration of trust. Strategy is
[#12](https://github.com/veridatum-labs/earnproof-contracts/issues/12).

**Assumption — testnet only.** `SECURITY.md` scopes the project to Stellar
testnet. No mainnet security claim is made. Several accepted risks above would
need to be re-classified before mainnet.

**Limitation — `mock_all_auths` in tests.** See the reviewer note under
[Entry points](#entry-points-and-privileges). Most existing tests do not prove
authorization; they prove functionality under permissive auth.

## Open gaps

Every gap links to a tracking issue. None is silently accepted.

| Gap | Impact on review confidence | Issue |
|---|---|---|
| **No authorization is proven anywhere** — every test runs under `mock_all_auths` | Removing any `require_auth` would fail no test. This is the single largest gap in the repository | [#34](https://github.com/veridatum-labs/earnproof-contracts/issues/34) |
| No adversarial pause or emergency-recovery tests | I3, I4, I8 and I20 unproven: admin rotation during pause, former-admin authority retention, and containment-tool availability are all read-only claims | [#44](https://github.com/veridatum-labs/earnproof-contracts/issues/44) |
| No mutation testing of auth and validation | Unknown whether existing tests would catch a removed check | [#38](https://github.com/veridatum-labs/earnproof-contracts/issues/38) |
| Panic strings, not stable typed errors | Callers cannot distinguish failure causes; no stable ABI | [#10](https://github.com/veridatum-labs/earnproof-contracts/issues/10) |
| `proof-registry` emits no events | No off-chain revocation timeline | [#3](https://github.com/veridatum-labs/earnproof-contracts/issues/3) |
| No reproducible WASM build provenance | Deployed bytecode unverified against source | [#17](https://github.com/veridatum-labs/earnproof-contracts/issues/17) |
| No TTL expiration and restoration boundary tests | The registry instance-TTL risk above is untested | [#35](https://github.com/veridatum-labs/earnproof-contracts/issues/35) |
| No cross-contract failure atomicity tests | Partial-state behaviour on callee failure unproven | [#36](https://github.com/veridatum-labs/earnproof-contracts/issues/36) |
| No fuzzing of shared type decoding | Malformed input handling unproven | [#37](https://github.com/veridatum-labs/earnproof-contracts/issues/37) |
| No per-contract invariant specifications | This index is the only invariant catalogue | [#39](https://github.com/veridatum-labs/earnproof-contracts/issues/39) |
| No deterministic ledger-time and expiration edge-case tests | Expiry boundary behaviour partially unproven | [#40](https://github.com/veridatum-labs/earnproof-contracts/issues/40) |
| No resource budget or WASM size regression gates | A change could exceed network limits undetected | [#19](https://github.com/veridatum-labs/earnproof-contracts/issues/19) |
| No ABI or storage compatibility golden tests | A storage layout change could go unnoticed | [#33](https://github.com/veridatum-labs/earnproof-contracts/issues/33) |
| Backend↔contract hashing vectors unpublished | Hash construction cannot be independently checked | [#43](https://github.com/veridatum-labs/earnproof-contracts/issues/43) |
| No governed upgrade or migration strategy | No documented response to a contract bug | [#12](https://github.com/veridatum-labs/earnproof-contracts/issues/12) |
| No coordinated admin rotation across contracts | Rotation is per-contract and can partially complete | [#11](https://github.com/veridatum-labs/earnproof-contracts/issues/11) |

## Verification commands

Run from the repository root. These are the same commands CI runs
(`.github/workflows/ci.yml`).

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo build --workspace
```

Test inventory at this commit:

| Suite | Path | Tests |
|---|---|---|
| Address validation | `tests/address-validation/src/lib.rs` | 5 |
| Authorization | `tests/authorization/src/lib.rs` | 21 |
| State machine | `tests/property/state_machine.rs` | 5 |
| Cross-contract atomicity | `tests/cross-contract/src/lib.rs` | 35 |
| Emergency and recovery | `tests/emergency/src/lib.rs` | 30 |
| Encoding vectors | `tests/encoding/src/lib.rs` | 5 |
| Error catalog | `tests/error-catalog/src/lib.rs` | 31 |
| Event fixtures | `tests/event-fixtures/src/lib.rs` | 13 |
| Event assertions | `tests/events/src/lib.rs` | 50 |
| `issuer-registry` | `contracts/issuer-registry/src/lib.rs` | 37 |
| Ledger snapshots | `tests/ledger-snapshots/src/lib.rs` | 8 |
| Ledger time | `tests/time/src/lib.rs` | 19 |
| `proof-registry` | `contracts/proof-registry/src/lib.rs` | 39 |
| `protocol-config` | `contracts/protocol-config/src/lib.rs` | 34 |
| Resource budgets | `tests/budgets/src/lib.rs` | 17 |
| Storage keys | `tests/storage-keys/src/lib.rs` | 21 |
| TTL | `tests/ttl/src/lib.rs` | 62 |
| **Total** | | **429** |

Manifest verification (PowerShell, no credentials required):

```powershell
.\scripts\verify-manifest.ps1 -Manifest scripts\deployment-manifest.testnet.json
.\scripts\verify-manifest.ps1 -Manifest scripts\deployment-manifest.example.json -AllowPlaceholders
```

Link and command validation for this index runs in CI:
`.github/workflows/security-evidence.yml`.

## Refresh checklist

This index is **commit-specific**. It is stale when any of the following is true.
A reviewer finding a stale index should request a refresh before proceeding.

- [ ] `HEAD` differs from `09f9841c9af78e67c90f0eaab1039052b17b9a03` and any file
      under `contracts/`, `packages/`, or `tests/` changed.
- [ ] A public entry point was added, removed, or renamed → refresh
      [Entry points](#entry-points-and-privileges) and its line numbers.
- [ ] Any line number in this document no longer points at what it claims. CI
      catches structural drift but not line-level drift; spot-check before an
      external handoff.
- [ ] An invariant was added, weakened, or newly tested → refresh
      [Invariants](#invariants).
- [ ] A `panic!` message changed or was added → refresh [Errors](#errors).
- [ ] An event was added or its fields changed → refresh [Events](#events) and
      `tests/fixtures/events/`.
- [ ] A storage key or TTL constant changed → refresh
      [Storage and TTL](#storage-and-ttl) and `docs/storage-model.md`.
- [ ] Contracts were redeployed → refresh
      [Artifact provenance](#artifact-provenance) from the new manifest.
- [ ] Any issue in [Open gaps](#open-gaps) closed, or a new gap opened.
- [ ] The `soroban-sdk` version or Rust toolchain changed.
- [ ] More than one release cycle has passed since the last refresh, regardless
      of whether anything above applies.

**Last refreshed:** at commit `09f9841`, against `soroban-sdk 27.0.0`.
