//! Restoration behaviour after archival.
//!
//! Since protocol 23 an archived *persistent* entry is restored automatically
//! when it is accessed, rather than the invocation being rejected. Restoration
//! is a state-preserving operation: the entry comes back byte for byte, with
//! the host minimum TTL, and it is billed as an extra ledger write plus a rent
//! bump. What restoration must never do is change who is allowed to act on the
//! restored record, or turn an expired credential back into a valid one. Those
//! are the properties this module pins down.

use super::fixture::{bytes, deployment, proof_key, COMMITMENT, FAR_FUTURE, SCHEMA_VERSION};
use earnproof_shared::{ProofError, ProofStatus, TTL_EXTEND_TO_LEDGERS};
use soroban_sdk::testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, IntoVal};

#[test]
fn restoration_returns_the_record_unchanged() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    let before = deployment.proofs.get_proof(&proof_id);

    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);
    let after = deployment.proofs.get_proof(&proof_id);

    assert_eq!(before, after);
}

#[test]
fn a_restoring_call_re_extends_the_entry_to_the_target() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);

    // `get_proof` restores (minimum TTL) and then applies the contract's own
    // extension policy on top of it.
    deployment.proofs.get_proof(&proof_id);

    assert_eq!(deployment.proof_ttl(&proof_id), TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn restoration_costs_an_extra_write_and_rent_bump() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);

    deployment.proofs.get_proof(&proof_id);
    let live = deployment.env.cost_estimate().resources();

    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);
    deployment.proofs.get_proof(&proof_id);
    let restoring = deployment.env.cost_estimate().resources();

    assert!(
        restoring.write_entries > live.write_entries,
        "restoring read must write more entries than a live read: {restoring:?} vs {live:?}"
    );
    assert!(
        restoring.persistent_entry_rent_bumps > live.persistent_entry_rent_bumps,
        "restoring read must bump more rent than a live read: {restoring:?} vs {live:?}"
    );
}

#[test]
fn an_expired_proof_is_not_valid_after_restoration() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(2_000);

    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);
    deployment.env.ledger().set_timestamp(2_001);

    assert!(!deployment.proofs.is_valid_proof(&proof_id));
    assert_eq!(
        deployment.proofs.get_proof(&proof_id).status,
        ProofStatus::Active
    );
}

#[test]
fn a_revoked_proof_stays_revoked_after_restoration() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    deployment.proofs.revoke_proof(&proof_id);
    let revoked_at = deployment.proofs.get_proof(&proof_id).revoked_at;

    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);

    assert!(deployment.proofs.is_revoked(&proof_id));
    assert!(!deployment.proofs.is_valid_proof(&proof_id));
    assert_eq!(
        deployment.proofs.get_proof(&proof_id).revoked_at,
        revoked_at
    );
    assert_eq!(
        deployment.proofs.try_revoke_proof(&proof_id),
        Err(Ok(ProofError::ProofAlreadyRevoked))
    );
}

#[test]
fn a_restored_proof_id_cannot_be_re_registered() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);

    // A new registration under an existing identifier must fail even though the
    // entry was archived a moment ago. Anything else would let an archived
    // commitment be replaced.
    let result = deployment.proofs.try_register_proof(
        &proof_id,
        &bytes(&deployment.env, 99),
        &deployment.issuer,
        &SCHEMA_VERSION,
        &FAR_FUTURE,
    );

    assert_eq!(result, Err(Ok(ProofError::ProofAlreadyRegistered)));
    assert_eq!(
        deployment.proofs.get_proof(&proof_id).commitment_hash,
        bytes(&deployment.env, COMMITMENT)
    );
}

#[test]
fn restoration_does_not_weaken_revocation_authorization() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);

    // Authorization is re-evaluated from scratch: a stranger cannot revoke the
    // restored proof, and the original issuer still can.
    let stranger = Address::generate(&deployment.env);
    deployment.env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &deployment.proofs_id,
            fn_name: "revoke_proof",
            args: (proof_id.clone(),).into_val(&deployment.env),
            sub_invokes: &[],
        },
    }]);
    assert!(deployment.proofs.try_revoke_proof(&proof_id).is_err());
    assert!(!deployment.proofs.is_revoked(&proof_id));

    deployment.env.mock_auths(&[MockAuth {
        address: &deployment.issuer,
        invoke: &MockAuthInvoke {
            contract: &deployment.proofs_id,
            fn_name: "revoke_proof",
            args: (proof_id.clone(),).into_val(&deployment.env),
            sub_invokes: &[],
        },
    }]);
    deployment.proofs.revoke_proof(&proof_id);
    assert!(deployment.proofs.is_revoked(&proof_id));
}

#[test]
fn a_long_idle_deployment_recovers_every_contract_on_the_next_call() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);

    // Twice the extension target with no traffic at all: every instance entry,
    // the issuer record, the reverse index and the schema flag are archived.
    deployment.idle(TTL_EXTEND_TO_LEDGERS * 2);

    // A registration exercises all three contracts in one invocation.
    let second_proof = bytes(&deployment.env, 7);
    deployment.proofs.register_proof(
        &second_proof,
        &bytes(&deployment.env, 8),
        &deployment.issuer,
        &SCHEMA_VERSION,
        &FAR_FUTURE,
    );

    assert!(deployment.proofs.is_valid_proof(&second_proof));
    assert!(deployment.proofs.is_valid_proof(&proof_id));
    // The proof entries were re-extended by the calls that touched them.
    assert_eq!(deployment.proof_ttl(&second_proof), TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn the_proof_registry_instance_is_only_extended_at_initialize() {
    let deployment = deployment();

    // `initialize` is the only proof-registry entry point that extends the
    // instance entry. Every later call restores it if needed but leaves it at
    // the host minimum, so a registry that is written to exactly once per
    // archival window keeps paying restoration costs. Operators who want the
    // instance to stay live must extend it out of band; see
    // `docs/storage-ttl.md`.
    assert_eq!(deployment.proof_instance_ttl(), TTL_EXTEND_TO_LEDGERS);

    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);
    deployment.register_proof(FAR_FUTURE);

    let minimum = deployment.env.ledger().get().min_persistent_entry_ttl;
    assert_eq!(deployment.proof_instance_ttl(), minimum - 1);
}

#[test]
fn restoration_preserves_the_proof_storage_key() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    deployment.idle(TTL_EXTEND_TO_LEDGERS + 1);
    deployment.proofs.get_proof(&proof_id);

    // The restored entry lives under the same key it was written to, so a
    // rename of the `DataKey` variant would surface here rather than as a
    // silent second entry.
    let key = proof_key(&deployment.env, &proof_id);
    let present = deployment.env.as_contract(&deployment.proofs_id, || {
        deployment.env.storage().persistent().has(&key)
    });
    assert!(present);
}
