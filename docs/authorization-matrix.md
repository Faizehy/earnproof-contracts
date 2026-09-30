# Contract Authorization Negative-Test Matrix

This document is the single source of truth for every mutating public entry
point across the three EarnProof contracts. The executable form lives in
`tests/authorization/`; every assertion in the test suite maps to a row below.
The guard constant `DOCUMENTED_MUTATIONS` in `matrix.rs` fails when the
documented count drifts from the code.

## Scope

Privileged entry points span:

- **protocol-config** — pause controls, admin rotation, schema lifecycle
- **issuer-registry** — issuer registration, status transitions, address rotation
- **proof-registry** — proof registration, issuer-gated revocation, admin revocation

Success-path tests do not prove every unauthorized identity is side-effect free.
This matrix does.

Critical-action governance adds an optional second authorization mode for
schema policy and dependency-replacement changes. When disabled, existing
admin authorization remains in effect. When enabled, direct mutation is
rejected; the admin proposes the exact action and configured signers approve
that contract-scoped proposal. Proposal approval and execution authorization
are covered by the contract-local governance tests, outside the fixed
`DOCUMENTED_MUTATIONS` matrix count below. Emergency pause controls remain
immediate admin mutations in either mode.

## Identity categories

| Label | Meaning |
|-------|---------|
| **Missing** | No authorization entry at all. |
| **Wrong (attacker)** | An unrelated address with no authority anywhere. |
| **Wrong (cross-role)** | A different active issuer or a different contract's admin — the realistic insider threat. |
| **Authorized** | The documented authority signs (control case). |
| **Former admin** | An address that *was* the admin before rotation. |
| **Stale issuer** | An address that was valid before `rotate_issuer_address`. |

## Matrix rows

### protocol-config

| Entry point | Required authority | Missing | Wrong | Authorized |
|---|---|---|---|---|
| `initialize` | New admin (self-signing) | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `set_admin` | Current admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `pause` | Current admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `unpause` | Current admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `approve_schema_version` | Current admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `deprecate_schema_version` | Current admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |

### issuer-registry

| Entry point | Required authority | Missing | Wrong | Authorized |
|---|---|---|---|---|
| `initialize` | New admin (self-signing) | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `register_issuer` | Admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `update_issuer` | Admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `suspend_issuer` | Admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `reactivate_issuer` | Admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `revoke_issuer` | Admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |
| `rotate_issuer_address` | Admin | ❌ Rejected | ❌ Rejected | ✅ Accepted |

### proof-registry

| Entry point | Required authority | Missing | Wrong (attacker) | Wrong (cross-role) | Authorized |
|---|---|---|---|---|---|
| `initialize` | New admin (self-signing) | ❌ Rejected | ❌ Rejected | — | ✅ Accepted |
| `register_proof` | Named issuer | ❌ Rejected | ❌ Rejected | Different active issuer ❌ Rejected | ✅ Accepted |
| `revoke_proof` | Proof's issuer | ❌ Rejected | ❌ Rejected | Different active issuer ❌ Rejected | ✅ Accepted |
| `admin_revoke_proof` | Registry admin | ❌ Rejected | ❌ Rejected | Proof's own issuer ❌ Rejected | ✅ Accepted |

## Read-only entry points (no authorization required)

These are intentionally unauthenticated. Indexers and verifiers depend on
them; gating them would break integrations.

| Contract | Entry point |
|----------|-------------|
| protocol-config | `get_admin`, `is_paused`, `get_config_version`, `is_schema_version_approved` |
| issuer-registry | `get_admin`, `get_issuer`, `get_issuer_by_address`, `is_active_issuer`, `is_active_address` |
| proof-registry | `get_admin`, `get_issuer_registry`, `get_protocol_config`, `get_proof`, `is_valid_proof`, `is_revoked` |

## Side-effect assertions

Every negative row asserts **both**:

1. The call is rejected (returns an error / panics under the host).
2. **Complete absence of side effects**: persistent storage, instance storage,
   instance TTL, persistent TTL, and events are identical before and after.

The snapshot captures the full persistent and instance storage of all three
contracts. A rejected call must not change a single byte.

## Cross-contract authorization boundaries

### Two revocation paths, two identities

`proof-registry` exposes two revocation entry points that demand different
identities:

- **`revoke_proof`** — signed by the proof's issuer address (the identity
  stored on the proof record).
- **`admin_revoke_proof`** — signed by the proof-registry admin.

The admin cannot use the issuer path. The issuer cannot use the admin path.
A *different active issuer* cannot use either path for someone else's proof.
These cross-role boundaries are asserted explicitly in `delegation.rs`.

### Independent per-contract admins

Each contract maintains its own admin record. Rotating `protocol-config`'s
admin does **not** move authority over `issuer-registry` or `proof-registry`.
The test `rotating_the_config_admin_does_not_move_registry_authority` pins
this invariant.

## Delegation and authorization trees

Soroban records the exact tree of `require_auth` calls each invocation
demands. These tests pin those trees:

- Every mutating entry point demands **exactly one** root signature from the
  documented authority.
- No entry point performs delegated invocation (forwarding a caller's auth
  through a sub-invocation).
- No entry point requires more than one signature.
- Cross-contract reads (`is_paused`, `is_schema_version_approved`,
  `is_active_address`) do **not** add authorization nodes.

## Stale and former-admin identities

| Scenario | Test | Expected |
|----------|------|----------|
| Former admin after rotation | `a_former_admin_retains_no_authority` | All mutations rejected; no side effects |
| Former admin tries to reclaim | `a_former_admin_cannot_reclaim_authority` | Rejected; successor stays in place |
| Rotation to incumbent | `rotation_to_the_incumbent_keeps_authority_intact` | Accepted; authority unchanged |
| Rotated-out issuer loses registration | `a_rotated_out_issuer_address_loses_issuer_status` | Old address rejected for `register_proof` |
| Rotated-out issuer keeps revocation | `a_rotated_out_issuer_address_keeps_revocation_authority` | Old address can still `revoke_proof` its historical proofs |
| Replacement cannot revoke historical | `the_replacement_address_cannot_revoke_historical_proofs` | New address rejected for old proofs |

## Contract state preconditions

Some rows require fixture state before the authorization attempt:

| Row | Precondition | Built by |
|-----|-------------|----------|
| `reactivate_issuer` | Issuer must be suspended first | `Deployment::suspend_issuer` |
| `revoke_proof` | Proof must be registered | `Deployment::register_proof` |
| `admin_revoke_proof` | Proof must be registered | `Deployment::register_proof` |
| `initialize` rows | Contracts must be uninitialized | `Deployment::uninitialized` |

## Synchronization with generated specs

The guard constant in `matrix.rs`:

```rust
const DOCUMENTED_MUTATIONS: usize = 17;
```

Bump this constant **only together with** this document when a new mutating
function is added. The test `matrix_covers_every_mutating_public_function`
fails immediately when the counts disagree.

## Summary statistics

| Metric | Count |
|--------|-------|
| Mutating entry points | 17 |
| Read-only entry points | 16 |
| Negative matrix rows (Missing × 17) | 17 |
| Negative matrix rows (Wrong × 17) | 17 |
| Control rows (Authorized × 17) | 17 |
| Cross-role boundary tests | 4 |
| Authorization tree assertions | 4 |
| Stale/former-admin tests | 6 |
| **Total authorization tests** | **21** |
