//! Extension-trigger boundaries.
//!
//! `extend_ttl(threshold, extend_to)` is a one-sided bump: the host rewrites
//! the entry's live-until ledger only when the remaining TTL is *at or below*
//! the threshold, and only when the new live-until is further out than the old
//! one. Both halves of that rule are pinned here, because a contract that
//! extended unconditionally would rewrite an entry on every read and pay rent
//! it does not owe, while one that never extended would silently archive.

use super::fixture::{
    address_issuer_key, bytes, deployment, issuer_key, schema_version_key, FAR_FUTURE, ISSUER_ID,
    SCHEMA_VERSION,
};
use earnproof_shared::{TTL_EXTEND_TO_LEDGERS, TTL_THRESHOLD_LEDGERS};
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};

#[test]
fn write_sets_persistent_ttl_to_the_extension_target() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    assert_eq!(deployment.proof_ttl(&proof_id), TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn initialize_sets_instance_ttl_to_the_extension_target_for_every_contract() {
    let deployment = deployment();

    for contract in [
        &deployment.config_id,
        &deployment.issuers_id,
        &deployment.proofs_id,
    ] {
        let ttl = deployment
            .env
            .as_contract(contract, || deployment.env.storage().instance().get_ttl());
        assert_eq!(ttl, TTL_EXTEND_TO_LEDGERS, "instance TTL for {contract:?}");
    }
}

#[test]
fn issuer_registration_extends_both_the_record_and_the_reverse_index() {
    let deployment = deployment();
    let issuer_id = bytes(&deployment.env, ISSUER_ID);

    let (record_ttl, index_ttl) = deployment.env.as_contract(&deployment.issuers_id, || {
        (
            deployment
                .env
                .storage()
                .persistent()
                .get_ttl(&issuer_key(&deployment.env, &issuer_id)),
            deployment
                .env
                .storage()
                .persistent()
                .get_ttl(&address_issuer_key(&deployment.env, &deployment.issuer)),
        )
    });

    assert_eq!(record_ttl, TTL_EXTEND_TO_LEDGERS);
    assert_eq!(index_ttl, TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn schema_approval_extends_the_schema_flag() {
    let deployment = deployment();

    let ttl = deployment.env.as_contract(&deployment.config_id, || {
        deployment
            .env
            .storage()
            .persistent()
            .get_ttl(&schema_version_key(&deployment.env, SCHEMA_VERSION))
    });

    assert_eq!(ttl, TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn a_read_one_ledger_above_the_threshold_does_not_extend() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    // Remaining TTL after the idle period is THRESHOLD + 1: one ledger short of
    // the trigger.
    deployment.idle(TTL_EXTEND_TO_LEDGERS - TTL_THRESHOLD_LEDGERS - 1);
    assert_eq!(deployment.proof_ttl(&proof_id), TTL_THRESHOLD_LEDGERS + 1);

    deployment.proofs.get_proof(&proof_id);

    assert_eq!(deployment.proof_ttl(&proof_id), TTL_THRESHOLD_LEDGERS + 1);
}

#[test]
fn a_read_exactly_at_the_threshold_extends_back_to_the_target() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    deployment.idle(TTL_EXTEND_TO_LEDGERS - TTL_THRESHOLD_LEDGERS);
    assert_eq!(deployment.proof_ttl(&proof_id), TTL_THRESHOLD_LEDGERS);

    deployment.proofs.get_proof(&proof_id);

    assert_eq!(deployment.proof_ttl(&proof_id), TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn repeated_extension_in_the_same_ledger_is_idempotent() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);
    deployment.idle(TTL_EXTEND_TO_LEDGERS - TTL_THRESHOLD_LEDGERS);

    for _ in 0..5 {
        deployment.proofs.get_proof(&proof_id);
        assert_eq!(deployment.proof_ttl(&proof_id), TTL_EXTEND_TO_LEDGERS);
    }
}

#[test]
fn extension_never_shortens_an_entry() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    // One ledger of idling, then a burst of reads. The entry is far above the
    // threshold, so its live-until must not move at all.
    deployment.idle(1);
    for _ in 0..10 {
        deployment.proofs.get_proof(&proof_id);
    }

    assert_eq!(deployment.proof_ttl(&proof_id), TTL_EXTEND_TO_LEDGERS - 1);
}

#[test]
fn periodic_reads_keep_an_entry_alive_across_a_long_lifetime() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    // Twenty extension cycles at the documented cadence: touch the entry once
    // per (target - threshold) ledgers. That is ten million ledgers of
    // simulated life, well past any single TTL window.
    let cadence = TTL_EXTEND_TO_LEDGERS - TTL_THRESHOLD_LEDGERS;
    for cycle in 0..20 {
        deployment.idle(cadence);
        deployment.proofs.get_proof(&proof_id);
        assert_eq!(
            deployment.proof_ttl(&proof_id),
            TTL_EXTEND_TO_LEDGERS,
            "cycle {cycle}"
        );
    }

    assert!(deployment.proofs.is_valid_proof(&proof_id));
}

#[test]
fn revocation_extends_the_proof_entry() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);
    deployment.idle(TTL_EXTEND_TO_LEDGERS - TTL_THRESHOLD_LEDGERS);

    deployment.proofs.revoke_proof(&proof_id);

    assert_eq!(deployment.proof_ttl(&proof_id), TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn no_contract_uses_temporary_storage() {
    let deployment = deployment();
    deployment.register_proof(FAR_FUTURE, None);
    deployment.proofs.revoke_proof(&bytes(&deployment.env, 5));
    deployment.config.pause();
    deployment.config.unpause();

    for contract in [
        &deployment.config_id,
        &deployment.issuers_id,
        &deployment.proofs_id,
    ] {
        assert_eq!(
            deployment.temporary_entry_count(contract),
            0,
            "temporary storage for {contract:?} must stay empty"
        );
    }
}
