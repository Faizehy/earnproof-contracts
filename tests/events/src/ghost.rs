//! Failed operations must never emit success-shaped events.
//!
//! This is the property indexers and backend reconciliation actually depend on.
//! An event that says `issuer_registered` when no issuer was registered is
//! worse than no event at all: the indexer commits a record that on-chain state
//! does not support, and nothing later contradicts it.
//!
//! Each test drives one documented failure class and asserts the invocation
//! published nothing. Every contract in this workspace signals rejection by
//! panicking, and a panicking invocation is rolled back — so the assertion is
//! that the rollback covers events, not only storage.

use crate::harness::{attempt_failure, hash, Deployment, APPROVED_SCHEMA};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Address;

/// Asserts that a rejected invocation published no events at all.
///
/// Stronger than checking for the absence of one topic: an operation that
/// failed should be indistinguishable, to an indexer, from one never attempted.
fn assert_silent(events: &[crate::harness::ObservedEvent], context: &str) {
    assert!(
        events.is_empty(),
        "{context}: a rejected operation emitted {} event(s); \
         failed calls must be silent so indexers cannot observe uncommitted state",
        events.len()
    );
}

// ─── Duplicate-registration failures ────────────────────────────────────────

#[test]
fn duplicate_issuer_id_emits_no_event() {
    let deployment = Deployment::new();
    let other = Address::generate(&deployment.env);

    let events = attempt_failure(&deployment, || {
        // `issuer_id` is already registered by the harness.
        deployment.issuers.register_issuer(
            &hash(&deployment.env, 0x01),
            &other,
            &hash(&deployment.env, 0xDD),
            &hash(&deployment.env, 0x99),
        );
    });

    assert_silent(&events, "duplicate issuer id");
}

#[test]
fn duplicate_issuer_address_emits_no_event() {
    let deployment = Deployment::new();
    let existing_address = deployment.issuer.clone();

    let events = attempt_failure(&deployment, || {
        deployment.issuers.register_issuer(
            &hash(&deployment.env, 0x09),
            &existing_address,
            &hash(&deployment.env, 0xDD),
            &hash(&deployment.env, 0x99),
        );
    });

    assert_silent(&events, "duplicate issuer address");
}

#[test]
fn duplicate_proof_id_emits_no_event() {
    let deployment = Deployment::new();
    let proof_id = deployment.register_proof(0x21);
    let expires = deployment.env.ledger().timestamp() + 100_000;

    let events = attempt_failure(&deployment, || {
        deployment.proofs.register_proof(
            &proof_id,
            &hash(&deployment.env, 0x22),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &expires,
        );
    });

    assert_silent(&events, "duplicate proof id");
}

#[test]
fn double_revocation_emits_no_event() {
    let deployment = Deployment::new();
    let proof_id = deployment.register_proof(0x23);
    deployment.proofs.admin_revoke_proof(&proof_id);

    let events = attempt_failure(&deployment, || {
        deployment.proofs.admin_revoke_proof(&proof_id);
    });

    assert_silent(&events, "second revocation");
}

// ─── Paused-protocol failures ───────────────────────────────────────────────

#[test]
fn registration_while_paused_emits_no_event() {
    // The cross-contract case: proof-registry reads the pause flag from
    // protocol-config and rejects. Neither contract may publish anything.
    let deployment = Deployment::new();
    deployment.config.pause();
    let expires = deployment.env.ledger().timestamp() + 100_000;

    let events = attempt_failure(&deployment, || {
        deployment.proofs.register_proof(
            &hash(&deployment.env, 0x31),
            &hash(&deployment.env, 0x32),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &expires,
        );
    });

    assert_silent(&events, "registration while paused");
}

// ─── Invalid-schema failures ────────────────────────────────────────────────

#[test]
fn unapproved_schema_emits_no_event() {
    let deployment = Deployment::new();
    let expires = deployment.env.ledger().timestamp() + 100_000;

    let events = attempt_failure(&deployment, || {
        deployment.proofs.register_proof(
            &hash(&deployment.env, 0x41),
            &hash(&deployment.env, 0x42),
            &deployment.issuer,
            &99, // never approved
            &expires,
        );
    });

    assert_silent(&events, "unapproved schema version");
}

#[test]
fn deprecated_schema_emits_no_event() {
    // A schema withdrawn after a caller built its transaction. The rejection
    // must be as silent as any other.
    let deployment = Deployment::new();
    deployment.config.deprecate_schema_version(&APPROVED_SCHEMA);
    let expires = deployment.env.ledger().timestamp() + 100_000;

    let events = attempt_failure(&deployment, || {
        deployment.proofs.register_proof(
            &hash(&deployment.env, 0x43),
            &hash(&deployment.env, 0x44),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &expires,
        );
    });

    assert_silent(&events, "deprecated schema version");
}

#[test]
fn zero_schema_version_emits_no_event() {
    let deployment = Deployment::new();

    let events = attempt_failure(&deployment, || {
        deployment.config.approve_schema_version(&0);
    });

    assert_silent(&events, "zero schema version");
}

// ─── Revoked-issuer failures ────────────────────────────────────────────────

#[test]
fn revoked_issuer_registration_emits_no_event() {
    let deployment = Deployment::new();
    deployment.issuers.revoke_issuer(
        &deployment.issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );
    let expires = deployment.env.ledger().timestamp() + 100_000;

    let events = attempt_failure(&deployment, || {
        deployment.proofs.register_proof(
            &hash(&deployment.env, 0x51),
            &hash(&deployment.env, 0x52),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &expires,
        );
    });

    assert_silent(&events, "registration by a revoked issuer");
}

#[test]
fn suspended_issuer_registration_emits_no_event() {
    let deployment = Deployment::new();
    deployment.issuers.suspend_issuer(
        &deployment.issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );
    let expires = deployment.env.ledger().timestamp() + 100_000;

    let events = attempt_failure(&deployment, || {
        deployment.proofs.register_proof(
            &hash(&deployment.env, 0x53),
            &hash(&deployment.env, 0x54),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &expires,
        );
    });

    assert_silent(&events, "registration by a suspended issuer");
}

#[test]
fn reactivating_a_revoked_issuer_emits_no_event() {
    // Revocation is terminal. A rejected reactivation that still published
    // `issuer_reactivated` would tell every indexer the issuer is trustworthy
    // again — the most damaging ghost event in this workspace.
    let deployment = Deployment::new();
    deployment.issuers.revoke_issuer(
        &deployment.issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );

    let events = attempt_failure(&deployment, || {
        deployment.issuers.reactivate_issuer(
            &deployment.issuer_id,
            &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
        );
    });

    assert_silent(&events, "reactivating a revoked issuer");
}

#[test]
fn updating_a_revoked_issuer_emits_no_event() {
    let deployment = Deployment::new();
    deployment.issuers.revoke_issuer(
        &deployment.issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );

    let events = attempt_failure(&deployment, || {
        deployment
            .issuers
            .update_issuer(&deployment.issuer_id, &hash(&deployment.env, 0xEE));
    });

    assert_silent(&events, "updating a revoked issuer");
}

#[test]
fn rotating_a_revoked_issuer_address_emits_no_event() {
    let deployment = Deployment::new();
    deployment.issuers.revoke_issuer(
        &deployment.issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );
    let replacement = Address::generate(&deployment.env);

    let events = attempt_failure(&deployment, || {
        deployment
            .issuers
            .rotate_issuer_address(&deployment.issuer_id, &replacement);
    });

    assert_silent(&events, "rotating a revoked issuer's address");
}

#[test]
fn rotating_to_a_taken_address_emits_no_event() {
    let deployment = Deployment::new();
    let second_id = hash(&deployment.env, 0x02);
    let second_address = Address::generate(&deployment.env);
    deployment.issuers.register_issuer(
        &second_id,
        &second_address,
        &hash(&deployment.env, 0xBB),
        &hash(&deployment.env, 0x99),
    );

    let events = attempt_failure(&deployment, || {
        // Rotating the first issuer onto the second issuer's address.
        deployment
            .issuers
            .rotate_issuer_address(&deployment.issuer_id, &second_address);
    });

    assert_silent(&events, "rotating onto a registered address");
}

// ─── Expiry failures ────────────────────────────────────────────────────────

#[test]
fn already_expired_proof_emits_no_event() {
    let deployment = Deployment::new();
    let past = deployment.env.ledger().timestamp() - 1;

    let events = attempt_failure(&deployment, || {
        deployment.proofs.register_proof(
            &hash(&deployment.env, 0x61),
            &hash(&deployment.env, 0x62),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &past,
        );
    });

    assert_silent(&events, "registration with a past expiry");
}

#[test]
fn expiry_equal_to_now_emits_no_event() {
    // The boundary: expiry must be strictly in the future.
    let deployment = Deployment::new();
    let now = deployment.env.ledger().timestamp();

    let events = attempt_failure(&deployment, || {
        deployment.proofs.register_proof(
            &hash(&deployment.env, 0x63),
            &hash(&deployment.env, 0x64),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &now,
        );
    });

    assert_silent(&events, "registration expiring at the current ledger time");
}

// ─── Missing-record failures ────────────────────────────────────────────────

#[test]
fn suspending_an_unknown_issuer_emits_no_event() {
    let deployment = Deployment::new();

    let events = attempt_failure(&deployment, || {
        deployment.issuers.suspend_issuer(
            &hash(&deployment.env, 0x7F),
            &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
        );
    });

    assert_silent(&events, "suspending an unknown issuer");
}

#[test]
fn revoking_an_unknown_proof_emits_no_event() {
    let deployment = Deployment::new();

    let events = attempt_failure(&deployment, || {
        deployment
            .proofs
            .admin_revoke_proof(&hash(&deployment.env, 0x7E));
    });

    assert_silent(&events, "revoking an unknown proof");
}

// ─── Re-initialization failures ─────────────────────────────────────────────

#[test]
fn reinitializing_protocol_config_emits_no_event() {
    // Re-initialization would reset the administrator. A ghost `initialized`
    // event here would tell an indexer the protocol had been redeployed.
    let deployment = Deployment::new();
    let attacker = Address::generate(&deployment.env);

    let events = attempt_failure(&deployment, || {
        deployment.config.initialize(&attacker);
    });

    assert_silent(&events, "re-initializing protocol-config");
}

#[test]
fn reinitializing_issuer_registry_emits_no_event() {
    let deployment = Deployment::new();
    let attacker = Address::generate(&deployment.env);

    let events = attempt_failure(&deployment, || {
        deployment.issuers.initialize(&attacker);
    });

    assert_silent(&events, "re-initializing issuer-registry");
}

// ─── State is unchanged after a rejected call ───────────────────────────────

#[test]
fn a_rejected_call_changes_neither_events_nor_storage() {
    // The two halves of the same guarantee. An indexer reconciling against
    // on-chain state must find them consistent: no event, and no change.
    let deployment = Deployment::new();
    let before = deployment.issuers.get_issuer(&deployment.issuer_id);
    let version_before = deployment.config.get_config_version();

    deployment.issuers.revoke_issuer(
        &deployment.issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );
    let after_revocation = deployment.issuers.get_issuer(&deployment.issuer_id);

    let events = attempt_failure(&deployment, || {
        deployment.issuers.reactivate_issuer(
            &deployment.issuer_id,
            &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
        );
    });

    assert_silent(&events, "rejected reactivation");
    assert_eq!(
        deployment.issuers.get_issuer(&deployment.issuer_id),
        after_revocation,
        "a rejected call must leave storage untouched"
    );
    assert_ne!(
        deployment.issuers.get_issuer(&deployment.issuer_id).status,
        before.status,
        "the earlier successful revocation should still be in effect"
    );
    assert_eq!(
        deployment.config.get_config_version(),
        version_before,
        "an issuer-registry failure must not advance the protocol config version"
    );
}
