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

#[test]
fn proof_registry_emits_no_events_as_documented() {
    // `tests/fixtures/events/proof-registry/v1/events.json` records that this
    // contract publishes nothing. That is the "unless explicitly documented
    // otherwise" case, and it is asserted rather than assumed: an indexer that
    // waited for a ProofRegistered event would wait forever, and this test is
    // what makes that a deliberate, visible decision.
    let deployment = Deployment::new();
    let events = deployment.capture(|| {
        let proof_id = deployment.register_proof(0x11);
        deployment.proofs.admin_revoke_proof(&proof_id);
    });
    let from_proof_registry: std::vec::Vec<_> = events
        .iter()
        .filter(|event| event.contract == deployment.proofs.address)
        .collect();

    assert!(
        from_proof_registry.is_empty(),
        "proof-registry is documented as emitting no events; \
         adding one requires updating tests/fixtures/events/proof-registry/ \
         and docs/events.md"
    );
}
