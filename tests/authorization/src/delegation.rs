//! Authorization trees and cross-contract boundaries.
//!
//! Soroban records the exact tree of `require_auth` calls each invocation
//! demands. These tests pin those trees: every mutating entry point must
//! demand exactly one root signature from the documented authority — no extra
//! signer, no delegated sub-invocation — and the cross-contract reads that
//! `proof-registry` performs must not add authorization nodes of their own.
//!
//! No entry point in this repository performs delegated invocation (a
//! contract forwarding a caller's `require_auth` through a sub-invocation),
//! and no entry point requires more than one signature. The single-root
//! assertions below make that explicit, so a future change that introduces a
//! sub-invocation or a second signer fails here instead of going unnoticed.
//!
//! The rejection tests at the bottom exercise the *boundaries* between the
//! two revocation paths and between contracts: the identity that is
//! authorized for one path or one contract is not authorized for the other.

use crate::harness::{authorize, hash, Deployment, APPROVED_SCHEMA};
use soroban_sdk::testutils::{AuthorizedFunction, AuthorizedInvocation};
use soroban_sdk::{Address, IntoVal, Symbol};

/// Asserts that the most recent invocation demanded exactly one authorization:
/// `signer` for `fn_name` on `contract` with `args`, and nothing else.
fn assert_single_auth_tree(
    d: &Deployment,
    signer: &Address,
    contract: &Address,
    fn_name: &str,
    args: soroban_sdk::Vec<soroban_sdk::Val>,
) {
    assert_eq!(
        d.env.auths(),
        [(
            signer.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    contract.clone(),
                    Symbol::new(&d.env, fn_name),
                    args,
                )),
                sub_invocations: std::vec::Vec::new(),
            }
        )],
        "{fn_name} demanded an unexpected authorization tree"
    );
}

#[test]
fn register_proof_demands_exactly_the_named_issuer() {
    // `register_proof` also calls `protocol-config::is_paused`,
    // `protocol-config::is_schema_version_approved`, and
    // `issuer-registry::is_active_address` along the way. None of those are
    // `require_auth` calls, so the tree must still contain exactly one node.
    let d = Deployment::new();
    d.env.mock_all_auths(); // recording mode: capture the demanded tree
    let proof_id = hash(&d.env, 0xA1);
    let commitment = hash(&d.env, 0xA2);
    let expires_at = d.env.ledger().timestamp() + 100_000;

    d.proofs.register_proof(
        &proof_id,
        &commitment,
        &d.issuer,
        &APPROVED_SCHEMA,
        &expires_at,
    );

    assert_single_auth_tree(
        &d,
        &d.issuer,
        &d.proofs_address,
        "register_proof",
        (
            &proof_id,
            &commitment,
            &d.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
        )
            .into_val(&d.env),
    );
}

#[test]
fn revoke_proof_demands_the_proofs_issuer() {
    let d = Deployment::new();
    let proof_id = d.register_proof(0xB1);
    d.env.mock_all_auths();

    d.proofs.revoke_proof(&proof_id);

    assert_single_auth_tree(
        &d,
        &d.issuer,
        &d.proofs_address,
        "revoke_proof",
        (&proof_id,).into_val(&d.env),
    );
}

#[test]
fn admin_revoke_proof_demands_the_registry_admin() {
    let d = Deployment::new();
    let proof_id = d.register_proof(0xB2);
    d.env.mock_all_auths();

    d.proofs.admin_revoke_proof(&proof_id);

    assert_single_auth_tree(
        &d,
        &d.admin,
        &d.proofs_address,
        "admin_revoke_proof",
        (&proof_id,).into_val(&d.env),
    );
}

#[test]
fn the_admin_cannot_revoke_through_the_issuer_path() {
    // `revoke_proof` and `admin_revoke_proof` demand different identities.
    // The registry admin is authorized for the admin path only.
    let d = Deployment::new();
    let proof_id = d.register_proof(0xC1);
    let before = d.snapshot();

    authorize(
        &d.env,
        &d.admin,
        &d.proofs_address,
        "revoke_proof",
        (&proof_id,).into_val(&d.env),
    );
    assert!(
        d.proofs.try_revoke_proof(&proof_id).is_err(),
        "the admin must not pass the issuer-gated revocation path"
    );
    d.assert_no_side_effects(&before, "admin on revoke_proof");
    assert!(!d.proofs.is_revoked(&proof_id));
}

#[test]
fn the_issuer_cannot_revoke_through_the_admin_path() {
    let d = Deployment::new();
    let proof_id = d.register_proof(0xC2);
    let before = d.snapshot();

    authorize(
        &d.env,
        &d.issuer,
        &d.proofs_address,
        "admin_revoke_proof",
        (&proof_id,).into_val(&d.env),
    );
    assert!(
        d.proofs.try_admin_revoke_proof(&proof_id).is_err(),
        "the proof's issuer must not pass the admin-gated path"
    );
    d.assert_no_side_effects(&before, "issuer on admin_revoke_proof");
    assert!(!d.proofs.is_revoked(&proof_id));
}

#[test]
fn a_different_active_issuer_cannot_revoke_someone_elses_proof() {
    // The realistic cross-issuer threat: `second_issuer` is a fully valid
    // active issuer with working credentials, but the proof belongs to
    // `issuer`. Its signature must not be accepted.
    let d = Deployment::new();
    let proof_id = d.register_proof(0xC3);
    let before = d.snapshot();

    // Establish the premise: the second issuer is a fully valid active issuer.
    assert!(d.issuers.is_active_issuer(&d.second_issuer_id));
    assert!(d.issuers.is_active_address(&d.second_issuer));

    authorize(
        &d.env,
        &d.second_issuer,
        &d.proofs_address,
        "revoke_proof",
        (&proof_id,).into_val(&d.env),
    );
    assert!(
        d.proofs.try_revoke_proof(&proof_id).is_err(),
        "a different active issuer must not revoke someone else's proof"
    );
    d.assert_no_side_effects(&before, "second issuer on revoke_proof");
    assert!(!d.proofs.is_revoked(&proof_id));
}

#[test]
fn registration_auth_is_not_forwardable_to_another_issuer() {
    // The named issuer in `register_proof` is the one whose signature is
    // demanded. Signing with a different issuer while naming the first one
    // must fail even though both are active issuers.
    let d = Deployment::new();
    let before = d.snapshot();
    let proof_id = hash(&d.env, 0xC4);
    let commitment = hash(&d.env, 0xC5);
    let expires_at = d.env.ledger().timestamp() + 100_000;

    authorize(
        &d.env,
        &d.second_issuer,
        &d.proofs_address,
        "register_proof",
        (
            &proof_id,
            &commitment,
            &d.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
        )
            .into_val(&d.env),
    );
    assert!(
        d.proofs
            .try_register_proof(
                &proof_id,
                &commitment,
                &d.issuer,
                &APPROVED_SCHEMA,
                &expires_at,
            )
            .is_err(),
        "issuer B must not register proofs in issuer A's name"
    );
    d.assert_no_side_effects(&before, "second issuer on register_proof");
}

#[test]
fn cross_contract_reads_leave_the_callees_untouched_on_rejection() {
    // When an unauthorized caller is rejected, the *callee* contracts that
    // `register_proof` consults must be untouched too: no read was admitted,
    // no TTL was extended, and their storage is identical.
    let d = Deployment::new();
    let before = d.snapshot();
    let proof_id = hash(&d.env, 0xC6);
    let commitment = hash(&d.env, 0xC7);
    let expires_at = d.env.ledger().timestamp() + 100_000;

    // No authorization at all.
    assert!(d
        .proofs
        .try_register_proof(
            &proof_id,
            &commitment,
            &d.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
        )
        .is_err());
    d.assert_no_side_effects(
        &before,
        "unauthorized register_proof across contract boundary",
    );

    // The callees still report their original state.
    assert!(!d.config.is_paused());
    assert!(d.config.is_schema_version_approved(&APPROVED_SCHEMA));
    assert!(d.issuers.is_active_address(&d.issuer));
}
