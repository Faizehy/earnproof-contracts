//! Expiry boundaries.
//!
//! An entry written at ledger `S` with an extension target of
//! `TTL_EXTEND_TO_LEDGERS` lives until ledger `S + TTL_EXTEND_TO_LEDGERS`
//! inclusive. The three ledgers that matter are therefore:
//!
//! | Ledger | Remaining TTL | State |
//! |---|---|---|
//! | `S + target - 1` | `1` | live |
//! | `S + target` | `0` | live, final ledger |
//! | `S + target + 1` | archived | restored on next access |
//!
//! The tests below sit on each of those ledgers. Archived-ness is observed
//! through the TTL the host reports immediately after the first access:
//! a live entry keeps whatever TTL it had, an archived one comes back with
//! `min_persistent_entry_ttl - 1`.

use super::fixture::{deployment, FAR_FUTURE};
use earnproof_shared::TTL_EXTEND_TO_LEDGERS;
use soroban_sdk::testutils::Ledger as _;

#[test]
fn entry_has_one_ledger_left_the_ledger_before_expiry() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    deployment.idle(TTL_EXTEND_TO_LEDGERS - 1);

    assert_eq!(deployment.proof_ttl(&proof_id), 1);
    assert!(deployment.proofs.is_valid_proof(&proof_id));
}

#[test]
fn entry_is_still_live_on_its_final_ledger() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    deployment.idle(TTL_EXTEND_TO_LEDGERS);

    // Remaining TTL of zero means "expires after this ledger", not "expired".
    // Reading it here must not restore anything.
    assert_eq!(deployment.proof_ttl(&proof_id), 0);
    assert!(deployment.proofs.is_valid_proof(&proof_id));
}

#[test]
fn a_read_on_the_final_ledger_extends_the_entry_back_to_the_target() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);
    deployment.idle(TTL_EXTEND_TO_LEDGERS);

    deployment.proofs.get_proof(&proof_id);

    assert_eq!(deployment.proof_ttl(&proof_id), TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn entry_is_archived_one_ledger_after_its_final_ledger() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);

    // The first access restores the entry with the host minimum TTL, which is
    // how an archived entry announces itself.
    let minimum = deployment.env.ledger().get().min_persistent_entry_ttl;
    assert_eq!(deployment.proof_ttl(&proof_id), minimum - 1);
}

#[test]
fn instance_storage_archives_on_the_same_boundary_as_persistent_storage() {
    let deployment = deployment();

    deployment.idle(TTL_EXTEND_TO_LEDGERS);
    assert_eq!(deployment.proof_instance_ttl(), 0);

    deployment.idle(1);
    let minimum = deployment.env.ledger().get().min_persistent_entry_ttl;
    assert_eq!(deployment.proof_instance_ttl(), minimum - 1);
}

#[test]
fn an_idle_contract_expires_because_nothing_extends_it_implicitly() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE, None);

    // Reads of *other* keys do not keep this entry alive. The protocol config
    // is touched throughout the idle window; the proof entry still expires.
    for _ in 0..5 {
        deployment.idle(TTL_EXTEND_TO_LEDGERS / 5);
        deployment.config.get_config_version();
    }
    deployment.idle(1);

    let minimum = deployment.env.ledger().get().min_persistent_entry_ttl;
    assert_eq!(deployment.proof_ttl(&proof_id), minimum - 1);
}

#[test]
fn timestamp_expiry_and_ttl_expiry_are_independent() {
    let deployment = deployment();
    // Expires by timestamp long before the storage entry could archive.
    let proof_id = deployment.register_proof(2_000, None);

    deployment.env.ledger().set_timestamp(2_001);

    // The entry is comfortably live in storage, and the proof is invalid.
    assert_eq!(deployment.proof_ttl(&proof_id), TTL_EXTEND_TO_LEDGERS);
    assert!(!deployment.proofs.is_valid_proof(&proof_id));
    // The record itself is still readable: expiry is a validity rule, not a
    // deletion.
    assert_eq!(deployment.proofs.get_proof(&proof_id).expires_at, 2_000);
}

#[test]
fn address_lookups_alone_do_not_keep_the_issuer_record_alive() {
    use super::fixture::{address_issuer_key, bytes, issuer_key, ISSUER_ID};
    use earnproof_shared::TTL_THRESHOLD_LEDGERS;
    use soroban_sdk::testutils::storage::Persistent as _;

    let deployment = deployment();
    let issuer_id = bytes(&deployment.env, ISSUER_ID);
    deployment.idle(TTL_EXTEND_TO_LEDGERS - TTL_THRESHOLD_LEDGERS);

    // `get_issuer_by_address` reads the record but only extends the reverse
    // index, so an indexer that never calls anything else lets the record
    // archive underneath a live index entry.
    deployment.issuers.get_issuer_by_address(&deployment.issuer);

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

    assert_eq!(record_ttl, TTL_THRESHOLD_LEDGERS);
    assert_eq!(index_ttl, TTL_EXTEND_TO_LEDGERS);

    // `is_active_address` walks through `is_active_issuer`, so it does keep
    // both entries alive.
    deployment.issuers.is_active_address(&deployment.issuer);
    let record_ttl = deployment.env.as_contract(&deployment.issuers_id, || {
        deployment
            .env
            .storage()
            .persistent()
            .get_ttl(&issuer_key(&deployment.env, &issuer_id))
    });
    assert_eq!(record_ttl, TTL_EXTEND_TO_LEDGERS);
}
