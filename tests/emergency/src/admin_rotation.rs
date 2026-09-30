//! Administrator authority during an incident.
//!
//! Two failure modes are modelled here, both of which turn a recoverable
//! incident into an unrecoverable one:
//!
//! * **Privilege retention** — a rotated-out administrator keeps the ability to
//!   change pause state, so containment can be undone by the party the rotation
//!   was meant to remove.
//! * **Silent stranding** — rotation succeeds against an address that can never
//!   authorise, leaving a paused contract with no one able to unpause it. The
//!   contract must not accept such a change without it being observable.

use crate::harness::Deployment;
use soroban_sdk::testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation};
use soroban_sdk::{Address, Symbol};

/// Asserts that the most recent invocation of `function` on `contract` was
/// authorised by `expected` and by nobody else.
///
/// `mock_all_auths` lets every call through, so the authorisation requirement
/// cannot be observed by watching for rejection. Instead the recorded auth tree
/// is inspected: it names the address the contract actually demanded.
fn assert_authorized_by(
    deployment: &Deployment,
    expected: &Address,
    contract: &Address,
    function: &str,
) {
    let auths = deployment.env.auths();
    let matching: std::vec::Vec<&(Address, AuthorizedInvocation)> = auths
        .iter()
        .filter(|(_, invocation)| match &invocation.function {
            AuthorizedFunction::Contract((invoked_contract, invoked_fn, _)) => {
                invoked_contract == contract
                    && *invoked_fn == Symbol::new(&deployment.env, function)
            }
            _ => false,
        })
        .collect();

    assert_eq!(
        matching.len(),
        1,
        "expected exactly one authorisation of {function}, found {}",
        matching.len()
    );
    assert_eq!(
        &matching[0].0, expected,
        "{function} was authorised by the wrong address"
    );
}

#[test]
fn pause_requires_the_current_admin() {
    let deployment = Deployment::new();
    let config_address = deployment.config.address.clone();

    deployment.config.pause();
    assert_authorized_by(&deployment, &deployment.admin, &config_address, "pause");
}

#[test]
fn unpause_requires_the_current_admin() {
    let deployment = Deployment::new();
    let config_address = deployment.config.address.clone();

    deployment.config.pause();
    deployment.config.unpause();
    assert_authorized_by(&deployment, &deployment.admin, &config_address, "unpause");
}

#[test]
fn pause_authority_follows_rotation_and_does_not_stay_with_the_former_admin() {
    let deployment = Deployment::new();
    let config_address = deployment.config.address.clone();
    let former_admin = deployment.admin.clone();
    let new_admin = Address::generate(&deployment.env);

    deployment.config.pause();
    deployment.config.nominate_admin(&new_admin);
    deployment.config.accept_admin();
    assert_eq!(deployment.config.get_admin(), new_admin);

    // The contract must now demand the new administrator's signature. If the
    // former admin were still accepted, the rotation would not have contained
    // anything — the removed party could unpause at will.
    deployment.config.unpause();
    assert_authorized_by(&deployment, &new_admin, &config_address, "unpause");

    deployment.config.pause();
    assert_authorized_by(&deployment, &new_admin, &config_address, "pause");

    assert_ne!(
        deployment.config.get_admin(),
        former_admin,
        "rotation must not leave authority with the former administrator"
    );
}

#[test]
fn rotation_does_not_clear_the_pause_flag() {
    // Rotating the administrator is an authority change, not a recovery action.
    // If it silently unpaused, an operator handing over control mid-incident
    // would re-open the contained operation without ever deciding to.
    let deployment = Deployment::new();
    let new_admin = Address::generate(&deployment.env);

    deployment.config.pause();
    deployment.config.nominate_admin(&new_admin);
    deployment.config.accept_admin();

    assert!(
        deployment.config.is_paused(),
        "admin rotation must leave containment in force"
    );
}

#[test]
fn a_former_admin_cannot_reclaim_authority_by_calling_set_admin() {
    let deployment = Deployment::new();
    let config_address = deployment.config.address.clone();
    let former_admin = deployment.admin.clone();
    let new_admin = Address::generate(&deployment.env);

    deployment.config.pause();
    deployment.config.nominate_admin(&new_admin);
    deployment.config.accept_admin();

    // The former admin attempts to rotate authority back to themselves. Under
    // `mock_all_auths` the call is not rejected, so the assertion is on who the
    // contract required: it must be the *current* admin, never the caller.
    deployment.config.nominate_admin(&former_admin);
    assert_authorized_by(&deployment, &new_admin, &config_address, "nominate_admin");
    deployment.config.accept_admin();
    assert_authorized_by(&deployment, &former_admin, &config_address, "accept_admin");
}

#[test]
fn every_rotation_is_observable_through_the_config_version() {
    // An operator reconstructing an incident needs to know that authority moved.
    // `config_version` is monotonic across every privileged mutation, so a gap
    // in it is evidence of a change the operator has not accounted for.
    let deployment = Deployment::new();
    let mut previous = deployment.config.get_config_version();

    for _ in 0..3 {
        let next_admin = Address::generate(&deployment.env);
        deployment.config.nominate_admin(&next_admin);
        deployment.config.accept_admin();

        let current = deployment.config.get_config_version();
        assert!(
            current > previous,
            "admin rotation must advance the config version"
        );
        previous = current;
    }
}

#[test]
fn rotation_to_the_incumbent_is_accepted_without_changing_authority() {
    // A retry of a rotation that already landed must be harmless. Operators
    // re-run commands under pressure; a self-rotation that stranded the contract
    // would be a trap.
    let deployment = Deployment::new();
    let admin = deployment.admin.clone();

    deployment.config.pause();
    deployment.config.nominate_admin(&admin);
    deployment.config.accept_admin();

    assert_eq!(deployment.config.get_admin(), admin);
    assert!(deployment.config.is_paused());

    // Authority is intact: the contract still works for the same administrator.
    deployment.config.unpause();
    assert!(!deployment.config.is_paused());
}

#[test]
fn registry_admins_are_independent_of_the_config_admin() {
    // The three contracts hold separate administrator records. Rotating the
    // protocol-config admin must not silently move authority over the
    // registries, or an operator would believe a single rotation contained more
    // than it did.
    let deployment = Deployment::new();
    let original = deployment.admin.clone();
    let new_admin = Address::generate(&deployment.env);

    deployment.config.nominate_admin(&new_admin);
    deployment.config.accept_admin();

    assert_eq!(deployment.config.get_admin(), new_admin);
    assert_eq!(
        deployment.issuers.get_admin(),
        original,
        "issuer-registry admin must not follow a protocol-config rotation"
    );
    assert_eq!(
        deployment.proofs.get_admin(),
        original,
        "proof-registry admin must not follow a protocol-config rotation"
    );
}

#[test]
fn admin_revocation_authority_follows_the_proof_registry_admin_only() {
    let deployment = Deployment::new();
    let proofs_address = deployment.proofs.address.clone();
    let proof_id = deployment.register_proof(0x91);
    let config_admin = Address::generate(&deployment.env);

    // Move the config admin, then confirm proof-registry still demands its own.
    deployment.config.nominate_admin(&config_admin);
    deployment.config.accept_admin();
    deployment.config.pause();

    deployment.proofs.admin_revoke_proof(&proof_id);
    assert_authorized_by(
        &deployment,
        &deployment.admin,
        &proofs_address,
        "admin_revoke_proof",
    );
}

#[test]
fn issuer_containment_requires_the_issuer_registry_admin() {
    let deployment = Deployment::new();
    let issuers_address = deployment.issuers.address.clone();
    let issuer_id = crate::harness::issuer_id_hash(&deployment.env, 1);

    deployment.config.pause();
    deployment.issuers.suspend_issuer(
        &issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );

    assert_authorized_by(
        &deployment,
        &deployment.admin,
        &issuers_address,
        "suspend_issuer",
    );
}

#[test]
fn rotating_an_issuer_address_during_pause_releases_the_old_mapping() {
    // Address rotation is a containment tool: it cuts a compromised key away
    // from an issuer identity. The old address must stop resolving, or the
    // compromised key would still be treated as an active issuer.
    let deployment = Deployment::new();
    let issuer_id = crate::harness::issuer_id_hash(&deployment.env, 1);
    let compromised = deployment.issuer.clone();
    let replacement = Address::generate(&deployment.env);

    deployment.config.pause();
    deployment
        .issuers
        .rotate_issuer_address(&issuer_id, &replacement);

    assert!(deployment.issuers.is_active_address(&replacement));
    assert!(
        !deployment.issuers.is_active_address(&compromised),
        "the rotated-out address must no longer resolve to an issuer"
    );
}

#[test]
fn a_revoked_issuer_cannot_be_reactivated_after_the_incident() {
    // Revocation is the terminal containment action. If it could be undone,
    // an operator could not rely on it as a hard stop.
    let deployment = Deployment::new();
    let issuer_id = crate::harness::issuer_id_hash(&deployment.env, 1);

    deployment.config.pause();
    deployment.issuers.revoke_issuer(
        &issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );
    deployment.config.unpause();

    assert!(
        deployment
            .issuers
            .try_reactivate_issuer(
                &issuer_id,
                &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32])
            )
            .is_err(),
        "revocation must survive the end of the pause"
    );
    assert!(!deployment.issuers.is_active_issuer(&issuer_id));
}

#[test]
fn a_revoked_issuer_cannot_register_new_proofs_after_unpause() {
    // The end-to-end containment property: pause stops the bleeding, issuer
    // revocation makes it permanent, and lifting the pause must not restore the
    // revoked issuer's ability to write.
    let deployment = Deployment::new();
    let issuer_id = crate::harness::issuer_id_hash(&deployment.env, 1);

    deployment.config.pause();
    deployment.issuers.revoke_issuer(
        &issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );
    deployment.config.unpause();

    assert!(
        deployment
            .proofs
            .try_register_proof(
                &crate::harness::hash(&deployment.env, 0xA1),
                &crate::harness::hash(&deployment.env, 0xA2),
                &deployment.issuer,
                &crate::harness::APPROVED_SCHEMA,
                &(deployment.env.ledger().timestamp() + 100_000),
            )
            .is_err(),
        "a revoked issuer must not regain write access when the pause lifts"
    );
}
