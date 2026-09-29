# TTL Storage Model Reference

## Overview

This document describes the Time-To-Live (TTL) model for Earnproof contracts built on Soroban SDK 27.0.0. All storage entries have associated TTL values that determine when they expire and are removed from the ledger. This reference is essential for operators and integrators to understand data lifecycle and restoration behavior.

## TTL Configuration Constants

All contracts use the same TTL thresholds defined in `packages/shared/src/lib.rs`:

| Constant | Value | Meaning |
|----------|-------|---------|
| `TTL_THRESHOLD_LEDGERS` | 50,000 | If entry TTL ≤ this, trigger extension |
| `TTL_EXTEND_TO_LEDGERS` | 500,000 | Extend TTL to this many ledgers from now |

### TTL Extension Semantics

When `extend_ttl(threshold, extend_to)` is called:
- Compares current entry TTL (ledgers remaining) to `threshold`
- If current TTL ≤ threshold, sets new TTL = `extend_to` ledgers from now
- Expiry ledger = current_ledger_sequence + extend_to

**Boundary condition**: Entry is expired when `current_ledger_sequence > expiry_ledger` (exclusive boundary)
- At `expiry_ledger`: entry is **still valid**
- At `expiry_ledger + 1`: entry is **expired**

Namespace ownership, key-collision safety, and the rules for adding a storage key are documented separately in [Storage Namespaces and Key Safety](./storage.md).

---

## Storage Entries by Contract

### Protocol Config Contract

#### Instance Storage (TTL managed collectively)

| Entry | DataKey | Access Pattern | Behavior on Expiry | Fail-Closed |
|-------|---------|-----------------|-------------------|------------|
| Admin | `Admin` | `get_admin()` always reads, never extends | NotInitialized error | ✓ Yes |
| Paused | `Paused` | Read-only; checked by `is_paused()` | Returns false | ✓ Yes |
| Config Version | `ConfigVersion` | Read-only; bumped on mutations | Returns 0 | ✓ Yes |
| Contract Version | `ContractVersion` | Read-only; version guard in upgrades | Returns 0 | ✓ Yes |
| Allowed WASM | `AllowedWasm(hash)` | `is_upgrade_allowed()` read-only | `has()` returns false | ✓ Yes |

**Instance TTL Extension**: Called via `extend_instance_ttl()` on:
- `initialize()`: extends on first setup
- `pause()`, `unpause()`: extends on config changes
- `set_admin()`: extends when admin changes
- `approve_upgrade()`, `upgrade_contract()`: extends on upgrade operations

#### Persistent Storage (Per-Schema-Version)

| Entry | DataKey | Access Pattern | Behavior on Expiry | Fail-Closed |
|-------|---------|-----------------|-------------------|------------|
| Approved Schema Version | `SchemaVersion(ver)` | `is_schema_version_approved(ver)` reads & extends | Returns false | ✓ Yes |

Soroban does not automatically extend TTLs. Every entry will expire and be archived unless a contract call explicitly extends it before the ledger cutoff. Archived persistent entries are restored automatically on the next access, at the cost of an extra ledger write and rent bump. The exact boundaries, restoration semantics, and operator cadence are documented in [Storage TTL, Expiration, and Restoration](./storage-ttl.md).
**Persistent TTL Extension**: Called via `extend_schema_ttl(version)` on:
- `approve_schema_version(ver)`: extends on approval
- `deprecate_schema_version(ver)`: extends on deprecation
- `is_schema_version_approved(ver)`: extends on every read (extend-on-read pattern)

---

### Issuer Registry Contract

#### Instance Storage (TTL managed collectively)

| Entry | DataKey | Access Pattern | Behavior on Expiry | Fail-Closed |
|-------|---------|-----------------|-------------------|------------|
| Admin | `Admin` | `get_admin()` always reads, never extends | NotInitialized error | ✓ Yes |
| Contract Version | `ContractVersion` | Read-only; version guard | Returns 0 | ✓ Yes |
| Allowed WASM | `AllowedWasm(hash)` | `is_upgrade_allowed()` read-only | `has()` returns false | ✓ Yes |

**Instance TTL Extension**: Called via `extend_instance_ttl()` on:
- `initialize()`: extends on setup
- `approve_upgrade()`, `upgrade_contract()`: extends on upgrades

#### Persistent Storage (Per-Issuer and Per-Address)

| Entry | DataKey | Access Pattern | Behavior on Expiry | Fail-Closed |
|-------|---------|-----------------|-------------------|------------|
| Issuer Record | `Issuer(hash)` | `get_issuer(hash)` reads & extends | IssuerNotFound error | ✓ Yes |
| Address → Issuer Mapping | `AddressIssuer(addr)` | `is_active_address(addr)`, `get_issuer_by_address(addr)` read & extend | Not found; returns false | ✓ Yes |

**Persistent TTL Extensions**: 
- `extend_issuer_ttl(hash)`: extends `Issuer(hash)` entry
- `extend_address_ttl(addr)`: extends `AddressIssuer(addr)` entry
- Called on:
  - `register_issuer()`: extends both Issuer and AddressIssuer
  - `get_issuer()`: extends Issuer entry on read
  - `update_issuer()`, `suspend_issuer()`, `reactivate_issuer()`, `revoke_issuer()`: extend Issuer entry
  - `rotate_issuer_address()`: extends both old entry (removed) and new entry
  - `get_issuer_by_address()`: extends AddressIssuer entry on read

**Critical**: Both `Issuer(hash)` and `AddressIssuer(addr)` for a single issuer are extended together. If one expires, the other will too (same operation). Both must be present for `is_active_address()` to succeed.

---

### Proof Registry Contract

#### Instance Storage (TTL managed collectively)

| Entry | DataKey | Access Pattern | Behavior on Expiry | Fail-Closed |
|-------|---------|-----------------|-------------------|------------|
| Admin | `Admin` | `get_admin()` always reads, never extends | NotInitialized error | ✓ Yes |
| Issuer Registry | `IssuerRegistry` | `get_issuer_registry()` always reads, never extends | NotInitialized error | ✓ Yes |
| Protocol Config | `ProtocolConfig` | `get_protocol_config()` always reads, never extends | NotInitialized error | ✓ Yes |
| Contract Version | `ContractVersion` | Read-only; version guard | Returns 0 | ✓ Yes |
| Allowed WASM | `AllowedWasm(hash)` | `is_upgrade_allowed()` read-only | `has()` returns false | ✓ Yes |

**Instance TTL Extension**: Called via `extend_instance_ttl()` on:
- `initialize()`: extends on setup
- `approve_upgrade()`, `upgrade_contract()`: extends on upgrades

#### Persistent Storage (Per-Proof)

| Entry | DataKey | Access Pattern | Behavior on Expiry | Fail-Closed |
|-------|---------|-----------------|-------------------|------------|
| Proof Record | `Proof(hash)` | `get_proof(hash)` reads & extends; `is_valid_proof(hash)` reads & extends | ProofNotFound error | ✓ Yes |
| Proof Context Commitments | `ProofContext(hash)` | `get_proof_context_commitments(hash)` reads & extends when present | Returns `None` for legacy records; ProofNotFound for an unknown proof | ✓ Yes |

**Persistent TTL Extension**: Called via `extend_proof_key_ttl(key)` on:
- `register_proof()`: extends on creation
- `get_proof()`: extends on read (extend-on-read pattern)
- `revoke_proof()`, `admin_revoke_proof()`: extend on revocation

`ProofContext(hash)` is a version-1 sidecar written only by the context-aware
registration methods. It stores the network, asset, and full context hashes,
not their preimages or private payment data. The serialized `ProofRecord`
shape is unchanged; records created before context commitments were added
remain readable, and their context query returns `None` to signal that their
network/asset policy was not recorded.

---

## Fail-Closed Pattern

All contracts implement a **fail-closed** pattern:

- **Reading expired storage returns an error/false, never silent defaults or stale data**
- `get_*()` functions return explicit error types (e.g., `IssuerNotFound`, `ProofNotFound`)
- Boolean check functions (e.g., `is_active_address()`) return false if storage is missing
- **No re-extend occurs on read of expired entries** — expiry is permanent until re-writing

### Why Fail-Closed?

- **Security**: Expired issuer or proof records should not silently succeed verification
- **Operational clarity**: Admins can distinguish "not found" from "expired" via error types
- **Consistency**: All contracts follow the same pattern, reducing operator confusion

---

## Cross-Contract Dependencies

### Proof Registry → Issuer Registry

Proof verification depends on issuer status:

```
is_valid_proof(proof_id):
  1. Fetch proof record (requires Proof storage to exist)
  2. Check proof.status == Active && proof.expires_at >= now
  3. Call issuer_registry.is_active_address(proof.issuer_address)
  4. Return true only if ALL checks pass
```

**Implication**: If issuer storage expires before proof storage:
- `is_valid_proof()` will fail immediately (issuer is inactive)
- Proof record may still exist but is unreachable/invalid

### Proof Registry → Protocol Config

Proof registration checks schema approval:

```
register_proof(..., schema_version, ...):
  1. Call protocol_config.is_schema_version_approved(schema_version)
  2. Fail if schema is not approved or storage expired
```

**Implication**: Expiring a schema version in protocol-config makes all proofs using that version invalid until the schema is re-approved.

---

## Restoration and Recovery

### What Happens When Storage Expires?

1. Entry becomes unreachable via normal contract calls
2. Returns error/false as if entry never existed
3. **Data is not deleted immediately** — ledger-maintained TTL controls removal
4. **Cannot restore by re-reading** — must write new entry or re-approve/re-register

### How to Restore

**For Protocol Config Schema Versions:**
```
Admin calls approve_schema_version(version)
→ Creates fresh SchemaVersion(version) entry with new TTL
```

**For Issuer Registry Entries:**
```
Admin calls register_issuer(new_issuer_hash, address, metadata)
→ Creates fresh Issuer and AddressIssuer entries with new TTL
```

**For Proof Registry Entries:**
```
Issuer calls register_proof(proof_id, commitment, ...)
→ Creates fresh Proof entry with new TTL
```

### Operational Implications

1. **Proactive Extension**: Contract code extends TTL on every read/write to keep active data alive
2. **Monitoring**: Operators should monitor TTL usage to identify stale entries before expiry
3. **Restoration Timeline**: After expiry (> 500,000 ledgers ≈ ~2-3 days at 5-sec blocks), admins must re-register/re-approve
4. **Cascading Expiry**: If parent contracts (issuer-registry, protocol-config) expire, dependent contracts (proof-registry) become non-functional

---

## Test Coverage

Boundary tests in `tests/ttl/` verify all entries at:

| Scenario | Ledger Position | Expected Behavior |
|----------|-----------------|------------------|
| Pre-expiry | `expiry_ledger - 1` | Entry readable, TTL extended |
| At-expiry | `expiry_ledger` | Entry readable (inclusive boundary) |
| Post-expiry | `expiry_ledger + 1` | Entry not found / error returned |
| Restoration | Write new/updated entry | Fresh TTL, entry valid again |

Tests exercise:
- Instance storage expiry across all entry types
- Persistent storage expiry per contract
- Cross-contract dependencies (proof → issuer, proof → protocol)
- TTL extension triggers and thresholds
- Fail-closed error semantics

---

## Debugging TTL Issues

The `proof-registry` contract holds references to `issuer-registry` and `protocol-config` in its instance storage. These addresses are read by `get_issuer_registry()` and `get_protocol_config()`, which do **not** extend the proof registry's instance TTL. `initialize` is in fact the only proof-registry entry point that extends the instance entry: `register_proof` extends the proof entry it writes, not the instance. A long-idle registry therefore relies on automatic restoration of its instance entry, as described in [Storage TTL, Expiration, and Restoration](./storage-ttl.md).
### Symptoms & Resolution

| Issue | Root Cause | Resolution |
|-------|-----------|-----------|
| "Entry not found" error | Likely expired TTL, not actual missing | Check ledger sequence; re-register entry if expired |
| Proof verification fails | Issuer expired first (dependency) | Re-register issuer; proofs remain valid but unreachable |
| Schema rejected in proof | Protocol-config schema version expired | Admin re-approves schema version |
| Contracts unresponsive after idle period | Instance storage (Admin, etc.) expired | Re-initialize contracts |

### Checking TTL Programmatically

In tests or debug context:
```rust
env.as_contract(&contract_address, || {
    let key = DataKey::Admin;
    let ttl = env.storage().instance().get_ttl(&key);
    println!("Remaining TTL ledgers: {}", ttl);
});
```

---

## Summary

- **TTL Model**: Soroban ledger-sequence-based expiry (500,000 ledger threshold)
- **Boundary**: Inclusive at expiry (still valid at boundary; expired after boundary)
- **Pattern**: Extend-on-read for persistent entries; instance entries all extended together
- **Fail-Closed**: All expiry returns error/false; no silent defaults
- **Dependencies**: Proof → Issuer Registry → Protocol Config (cascading failures on expiry)
- **Recovery**: Admin re-registers/re-approves after expiry; no in-place restoration

For operational runbooks, see `docs/emergency-operations.md`.
