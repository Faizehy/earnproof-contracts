//! Every successful mutation emits its documented event exactly once.
//!
//! The existing per-contract tests count events. Counting cannot distinguish
//! "emitted the documented event" from "emitted some event", so these assert
//! the topic and then check the payload against committed storage — which is
//! the property an indexer actually depends on.

use crate::harness::{expect_single, hash, read_events, Deployment, APPROVED_SCHEMA};
use earnproof_shared::IssuerStatus;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};

// ─── protocol-config ────────────────────────────────────────────────────────

#[test]
fn initialize_emits_initialized_once() {
    // Built here rather than through the shared harness: the environment
    // reports only the most recent invocation, so by the time `Deployment::new`
    // returns, the initialization events have already been replaced by the
    // setup calls that followed them.
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(ProtocolConfigContract, ());
    let config = ProtocolConfigContractClient::new(&env, &contract_id);

    config.initialize(&admin);

    let events = read_events(&env);
    let event = expect_single(&env, &events, "initialized");

    // The payload names the administrator, and it must be the one installed.
    let announced: Address = event
        .field(&env, "admin")
        .expect("initialized event must carry an admin field");
    assert_eq!(announced, admin);
    assert_eq!(announced, config.get_admin());
}

#[test]
fn issuer_registry_initialize_emits_no_event() {
    // Only protocol-config announces initialization. The issuer registry does
    // not, and an indexer waiting for one would stall on deployment. Asserted
    // so the asymmetry is a recorded decision rather than an oversight.
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(IssuerRegistryContract, ());
    let registry = IssuerRegistryContractClient::new(&env, &contract_id);

    registry.initialize(&admin);

    assert!(
        read_events(&env).is_empty(),
        "issuer-registry initialization is documented as silent"
    );
}

#[test]
fn pause_emits_paused_once_and_matches_state() {
    let deployment = Deployment::new();
    let events = deployment.capture(|| deployment.config.pause());
    let event = expect_single(&deployment.env, &events, "paused");

    let flag: bool = event
        .field(&deployment.env, "paused")
        .expect("paused event must carry a paused field");

    // The payload must agree with committed storage, not merely be present.
    assert!(flag);
    assert_eq!(flag, deployment.config.is_paused());
}

#[test]
fn unpause_emits_unpaused_once_and_matches_state() {
    let deployment = Deployment::new();
    deployment.config.pause();
    let events = deployment.capture(|| deployment.config.unpause());
    let event = expect_single(&deployment.env, &events, "unpaused");

    let flag: bool = event
        .field(&deployment.env, "paused")
        .expect("unpaused event must carry a paused field");
    assert!(!flag);
    assert_eq!(flag, deployment.config.is_paused());
}

#[test]
fn set_admin_emits_admin_changed_once_and_matches_state() {
    let deployment = Deployment::new();
    let successor = Address::generate(&deployment.env);
    let events = deployment.capture(|| {
        deployment.config.nominate_admin(&successor);
        deployment.config.accept_admin()
    });
    let event = expect_single(&deployment.env, &events, "admin_transfer_accepted");

    let announced: Address = event
        .field(&deployment.env, "new_admin")
        .expect("admin_transfer_accepted event must carry a new_admin field");

    assert_eq!(announced, successor);
    assert_eq!(announced, deployment.config.get_admin());
}

#[test]
fn approve_schema_version_emits_schema_approved_once() {
    let deployment = Deployment::new();
    let events = deployment.capture(|| deployment.config.approve_schema_version(&7));
    let event = expect_single(&deployment.env, &events, "schema_approved");

    let version: u32 = event
        .field(&deployment.env, "version")
        .expect("schema_approved event must carry a version field");
    assert_eq!(version, 7);
    assert!(deployment.config.is_schema_version_approved(&7));
}

#[test]
fn deprecate_schema_version_emits_schema_deprecated_once() {
    let deployment = Deployment::new();
    let events =
        deployment.capture(|| deployment.config.deprecate_schema_version(&APPROVED_SCHEMA));
    let event = expect_single(&deployment.env, &events, "schema_deprecated");

    let version: u32 = event
        .field(&deployment.env, "version")
        .expect("schema_deprecated event must carry a version field");
    assert_eq!(version, APPROVED_SCHEMA);
    assert!(!deployment
        .config
        .is_schema_version_approved(&APPROVED_SCHEMA));
}

// ─── issuer-registry ────────────────────────────────────────────────────────

#[test]
fn register_issuer_emits_issuer_registered_once_and_matches_storage() {
    let deployment = Deployment::new();
    let next = Address::generate(&deployment.env);
    let issuer_id = hash(&deployment.env, 0x02);
    let metadata = hash(&deployment.env, 0xBB);
    let events = deployment.capture(|| {
        deployment
            .issuers
            .register_issuer(&issuer_id, &next, &metadata, &metadata)
    });
    let event = expect_single(&deployment.env, &events, "issuer_registered");

    let record = deployment.issuers.get_issuer(&issuer_id);

    // Each announced field must equal what was actually written.
    let announced_id: BytesN<32> = event.field(&deployment.env, "issuer_id_hash").unwrap();
    let announced_address: Address = event.field(&deployment.env, "issuer_address").unwrap();
    let announced_metadata: BytesN<32> = event.field(&deployment.env, "metadata_hash").unwrap();
    let announced_created: u64 = event.field(&deployment.env, "created_at").unwrap();

    assert_eq!(announced_id, record.issuer_id_hash);
    assert_eq!(announced_address, record.issuer_address);
    assert_eq!(announced_metadata, record.metadata_hash);
    assert_eq!(announced_created, record.created_at);
}

#[test]
fn update_issuer_emits_issuer_metadata_updated_once_and_matches_storage() {
    let deployment = Deployment::new();
    let metadata = hash(&deployment.env, 0xCC);
    let events = deployment.capture(|| {
        deployment
            .issuers
            .update_issuer(&deployment.issuer_id, &metadata)
    });
    let event = expect_single(&deployment.env, &events, "issuer_metadata_updated");

    let record = deployment.issuers.get_issuer(&deployment.issuer_id);
    let announced_metadata: BytesN<32> = event.field(&deployment.env, "metadata_hash").unwrap();
    let announced_updated: u64 = event.field(&deployment.env, "updated_at").unwrap();

    assert_eq!(announced_metadata, record.metadata_hash);
    assert_eq!(announced_updated, record.updated_at);
    assert_eq!(announced_metadata, metadata);
}

#[test]
fn suspend_issuer_emits_issuer_suspended_once_and_matches_storage() {
    let deployment = Deployment::new();
    let events = deployment.capture(|| {
        deployment.issuers.suspend_issuer(
            &deployment.issuer_id,
            &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
        )
    });
    let event = expect_single(&deployment.env, &events, "issuer_suspended");

    let record = deployment.issuers.get_issuer(&deployment.issuer_id);
    assert_eq!(record.status, IssuerStatus::Suspended);

    let announced: u64 = event.field(&deployment.env, "updated_at").unwrap();
    assert_eq!(announced, record.updated_at);
}

#[test]
fn reactivate_issuer_emits_issuer_reactivated_once_and_matches_storage() {
    let deployment = Deployment::new();
    deployment.issuers.suspend_issuer(
        &deployment.issuer_id,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );
    let events = deployment.capture(|| {
        deployment.issuers.reactivate_issuer(
            &deployment.issuer_id,
            &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
        )
    });
    let event = expect_single(&deployment.env, &events, "issuer_reactivated");

    let record = deployment.issuers.get_issuer(&deployment.issuer_id);
    assert_eq!(record.status, IssuerStatus::Active);

    let announced: u64 = event.field(&deployment.env, "updated_at").unwrap();
    assert_eq!(announced, record.updated_at);
}

#[test]
fn revoke_issuer_emits_issuer_revoked_once_and_matches_storage() {
    let deployment = Deployment::new();
    let events = deployment.capture(|| {
        deployment.issuers.revoke_issuer(
            &deployment.issuer_id,
            &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
        )
    });
    let event = expect_single(&deployment.env, &events, "issuer_revoked");

    let record = deployment.issuers.get_issuer(&deployment.issuer_id);
    assert_eq!(record.status, IssuerStatus::Revoked);

    let announced: u64 = event.field(&deployment.env, "updated_at").unwrap();
    assert_eq!(announced, record.updated_at);
}

#[test]
fn rotate_issuer_address_emits_both_old_and_new_address() {
    // The rotation event carries both addresses so an indexer can update its
    // mapping without scanning storage. Both must be present and correct, or
    // the indexer silently keeps routing to a rotated-out key.
    let deployment = Deployment::new();
    let replacement = Address::generate(&deployment.env);
    let previous = deployment.issuer.clone();
    let events = deployment.capture(|| {
        deployment
            .issuers
            .rotate_issuer_address(&deployment.issuer_id, &replacement)
    });
    let event = expect_single(&deployment.env, &events, "issuer_address_rotated");

    let announced_old: Address = event.field(&deployment.env, "old_address").unwrap();
    let announced_new: Address = event.field(&deployment.env, "new_address").unwrap();

    assert_eq!(announced_old, previous);
    assert_eq!(announced_new, replacement);

    let record = deployment.issuers.get_issuer(&deployment.issuer_id);
    assert_eq!(record.issuer_address, replacement);
}

// ─── proof-registry ─────────────────────────────────────────────────────────
//
// proof-registry used to emit nothing (tracked as the known gap in
// docs/events.md and issue #3). Exposing the registry epoch through typed
// events, as required by issue #187, closes that gap for register_proof and
// revoke_proof/admin_revoke_proof: each now publishes exactly one event
// carrying the epoch it advanced to, matching the "at most one event per
// invocation" rule the rest of this workspace follows.

#[test]
fn register_proof_emits_proof_registered_with_the_advanced_epoch() {
    let deployment = Deployment::new();
    let epoch_before = deployment.proofs.get_registry_epoch();
    let proof_id = hash(&deployment.env, 0x11);
    let expires_at = deployment.env.ledger().timestamp() + 100_000;

    let events = deployment.capture(|| {
        deployment.proofs.register_proof(
            &proof_id,
            &hash(&deployment.env, 0xEE),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
            &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
        );
    });

    let event = expect_single(&deployment.env, &events, "proof_registered");
    let announced_id: BytesN<32> = event
        .field(&deployment.env, "proof_id_hash")
        .expect("proof_registered event must carry proof_id_hash");
    let announced_epoch: u32 = event
        .field(&deployment.env, "epoch")
        .expect("proof_registered event must carry epoch");
    assert_eq!(announced_id, proof_id);
    assert_eq!(announced_epoch, epoch_before + 1);
    assert_eq!(deployment.proofs.get_registry_epoch(), epoch_before + 1);
}

#[test]
fn revoke_proof_emits_timing_and_the_advanced_epoch() {
    let deployment = Deployment::new();
    let proof_id = deployment.register_proof(0x21);
    let epoch_before = deployment.proofs.get_registry_epoch();

    let events = deployment.capture(|| deployment.proofs.revoke_proof(&proof_id));
    let event = expect_single(&deployment.env, &events, "proof_revoked");
    let announced_id: BytesN<32> = event.field(&deployment.env, "proof_id_hash").unwrap();
    let revoked_at: u64 = event.field(&deployment.env, "revoked_at").unwrap();
    let revoked_ledger: u32 = event.field(&deployment.env, "revoked_ledger").unwrap();
    let by_admin: bool = event.field(&deployment.env, "by_admin").unwrap();
    let announced_epoch: u32 = event.field(&deployment.env, "epoch").unwrap();
    let record = deployment.proofs.get_proof(&proof_id);

    assert_eq!(announced_id, proof_id);
    assert_eq!(revoked_at, record.revoked_at);
    assert_eq!(revoked_ledger, record.revoked_ledger);
    assert!(!by_admin);
    assert_eq!(announced_epoch, epoch_before + 1);
}

#[test]
fn admin_revoke_proof_emits_timing_and_admin_flag() {
    let deployment = Deployment::new();
    let proof_id = deployment.register_proof(0x22);

    let events = deployment.capture(|| deployment.proofs.admin_revoke_proof(&proof_id));
    let event = expect_single(&deployment.env, &events, "proof_revoked");
    let by_admin: bool = event.field(&deployment.env, "by_admin").unwrap();
    let record = deployment.proofs.get_proof(&proof_id);
    let revoked_at: u64 = event.field(&deployment.env, "revoked_at").unwrap();
    let revoked_ledger: u32 = event.field(&deployment.env, "revoked_ledger").unwrap();

    assert!(by_admin);
    assert_eq!(revoked_at, record.revoked_at);
    assert_eq!(revoked_ledger, record.revoked_ledger);
}

#[test]
fn a_rejected_registration_publishes_no_event_and_does_not_advance_the_epoch() {
    let deployment = Deployment::new();
    deployment.config.pause();
    let epoch_before = deployment.proofs.get_registry_epoch();
    let expires_at = deployment.env.ledger().timestamp() + 100_000;

    let events = deployment.capture(|| {
        let _ = deployment.proofs.try_register_proof(
            &hash(&deployment.env, 0x31),
            &hash(&deployment.env, 0x32),
            &deployment.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
            &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
        );
    });

    assert!(events.is_empty(), "a rejected call must publish nothing");
    assert_eq!(deployment.proofs.get_registry_epoch(), epoch_before);
}

#[test]
fn each_successful_proof_mutation_emits_one_proof_registry_event() {
    let deployment = Deployment::new();
    let registration_events = deployment.capture(|| deployment.register_proof(0x23));
    let proof_events: std::vec::Vec<_> = registration_events
        .iter()
        .filter(|event| event.contract == deployment.proofs.address)
        .collect();
    assert_eq!(proof_events.len(), 1);
    assert!(proof_events[0].is(&deployment.env, "proof_registered"));

    let proof_id = deployment.register_proof(0x24);
    let revocation_events = deployment.capture(|| deployment.proofs.admin_revoke_proof(&proof_id));
    let proof_events: std::vec::Vec<_> = revocation_events
        .iter()
        .filter(|event| event.contract == deployment.proofs.address)
        .collect();
    assert_eq!(proof_events.len(), 1);
    assert!(proof_events[0].is(&deployment.env, "proof_revoked"));
}