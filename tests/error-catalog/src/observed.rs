//! The codes the contracts actually return.
//!
//! A catalog that describes intended behaviour is worse than no catalog: a
//! backend written against it fails in production in ways nobody predicted.
//! Every entry marked `Returned` is driven here through a real failure path,
//! and the code that comes back is compared against the catalog. Every entry
//! marked `Reserved` is asserted to be absent from all of those paths.

use earnproof_shared::error_catalog::Status;
use earnproof_shared::{ContractError, InterfaceVersion, IssuerError, ProofError, ERROR_CATALOG};
use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};

const FAR_FUTURE: u64 = 10_000_000;

/// A stand-in issuer registry that reports an interface version the proof
/// registry cannot bind to. Used to drive the incompatible-dependency path.
#[contract]
pub struct BadVersionRegistry;

#[contractimpl]
impl BadVersionRegistry {
    pub fn is_active_address(_env: Env, _issuer_address: Address) -> bool {
        true
    }

    pub fn interface_version(_env: Env) -> InterfaceVersion {
        // A different major is a breaking-change boundary the consumer rejects.
        InterfaceVersion::new(99, 0, 0)
    }
}

fn bytes32(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}

struct Deployment {
    env: Env,
    config: ProtocolConfigContractClient<'static>,
    issuers: IssuerRegistryContractClient<'static>,
    proofs: ProofRegistryContractClient<'static>,
    issuers_id: Address,
    config_id: Address,
    admin: Address,
    issuer: Address,
}

fn deployment() -> Deployment {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);

    let config_id = env.register(ProtocolConfigContract, ());
    let config = ProtocolConfigContractClient::new(&env, &config_id);
    config.initialize(&admin);
    config.approve_schema_version(&1);
    config.approve_proof_type(&bytes32(&env, 1));

    let issuers_id = env.register(IssuerRegistryContract, ());
    let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
    issuers.initialize(&admin);
    issuers.register_issuer(
        &bytes32(&env, 1),
        &issuer,
        &bytes32(&env, 2),
        &bytes32(&env, 99),
    );

    let proofs_id = env.register(ProofRegistryContract, ());
    let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
    proofs.initialize(&admin, &issuers_id, &config_id);

    Deployment {
        env,
        config,
        issuers,
        proofs,
        issuers_id,
        config_id,
        admin,
        issuer,
    }
}

/// Records the code observed on one failure path, so that the full set can be
/// compared against the catalog at the end.
struct Observations {
    codes: std::vec::Vec<u32>,
}

impl Observations {
    fn new() -> Self {
        Self {
            codes: std::vec::Vec::new(),
        }
    }

    fn record(&mut self, path: &str, code: u32) {
        let entry = ERROR_CATALOG
            .into_iter()
            .find(|entry| entry.code == code)
            .unwrap_or_else(|| std::panic!("{path} returned uncatalogued code {code}"));
        assert_eq!(
            entry.status,
            Status::Returned,
            "{path} returned {}, which the catalog marks reserved",
            entry.name
        );
        self.codes.push(code);
    }
}

#[test]
fn every_returned_code_is_produced_by_a_real_failure_path() {
    let mut observed = Observations::new();

    // --- protocol-config -------------------------------------------------
    let initial_dep = deployment();
    let env = &initial_dep.env;

    let zero_address = Address::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    observed.record(
        "issuer-registry rejects sentinel governance address",
        code(initial_dep.issuers.try_grant_governance_role(
            &bytes32(env, 0xA1),
            &earnproof_shared::GovernanceRole::IssuerManagement,
            &zero_address,
            &env.ledger().sequence(),
            &None,
        )),
    );
    let now_ledger = env.ledger().sequence();
    observed.record(
        "issuer-registry rejects invalid governance timing",
        code(initial_dep.issuers.try_grant_governance_role(
            &bytes32(env, 0xA2),
            &earnproof_shared::GovernanceRole::IssuerManagement,
            &Address::generate(env),
            &(now_ledger + 2),
            &Some(now_ledger + 1),
        )),
    );
    observed.record(
        "issuer-registry rejects empty provenance commitment",
        code(initial_dep.issuers.try_register_issuer(
            &bytes32(env, 0xA3),
            &Address::generate(env),
            &bytes32(env, 0xA4),
            &soroban_sdk::BytesN::from_array(env, &[0; 32]),
        )),
    );

    // Exercise every timed-upgrade rejection while isolating its approval
    // record and ledger window from the other attempts.
    let no_approval = deployment();
    observed.record(
        "issuer-registry upgrade without approval",
        code(no_approval
            .issuers
            .try_upgrade_contract(&bytes32(&no_approval.env, 0xB1), &2)),
    );

    let early_upgrade = deployment();
    let early_hash = bytes32(&early_upgrade.env, 0xB2);
    early_upgrade
        .issuers
        .approve_upgrade(&bytes32(&early_upgrade.env, 0xB3), &early_hash, &2);
    observed.record(
        "issuer-registry upgrade before timelock",
        code(early_upgrade
            .issuers
            .try_upgrade_contract(&early_hash, &2)),
    );

    let expired_upgrade = deployment();
    let expired_hash = bytes32(&expired_upgrade.env, 0xB4);
    expired_upgrade.issuers.approve_upgrade(
        &bytes32(&expired_upgrade.env, 0xB5),
        &expired_hash,
        &2,
    );
    let expiry_ledger = expired_upgrade.env.ledger().sequence()
        + earnproof_shared::UPGRADE_APPROVAL_EXPIRY_LEDGERS;
    expired_upgrade
        .env
        .ledger()
        .set_sequence_number(expiry_ledger);
    observed.record(
        "issuer-registry expired upgrade approval",
        code(expired_upgrade
            .issuers
            .try_upgrade_contract(&expired_hash, &2)),
    );

    let mismatched_upgrade = deployment();
    let approved_hash = bytes32(&mismatched_upgrade.env, 0xB6);
    mismatched_upgrade.issuers.approve_upgrade(
        &bytes32(&mismatched_upgrade.env, 0xB7),
        &approved_hash,
        &2,
    );
    let executable_ledger = mismatched_upgrade.env.ledger().sequence()
        + earnproof_shared::UPGRADE_TIMELOCK_LEDGERS;
    mismatched_upgrade
        .env
        .ledger()
        .set_sequence_number(executable_ledger);
    observed.record(
        "issuer-registry upgrade hash mismatch",
        code(mismatched_upgrade
            .issuers
            .try_upgrade_contract(&bytes32(&mismatched_upgrade.env, 0xB8), &2)),
    );

    let fresh_config = env.register(ProtocolConfigContract, ());
    let fresh_config = ProtocolConfigContractClient::new(env, &fresh_config);
    observed.record(
        "protocol-config get_admin uninitialized",
        code(fresh_config.try_get_admin()),
    );
    observed.record(
        "protocol-config pause uninitialized",
        code(fresh_config.try_pause()),
    );
    observed.record(
        "protocol-config initialize twice",
        code(initial_dep.config.try_initialize(&initial_dep.admin)),
    );
    observed.record(
        "protocol-config approve_schema_version(0)",
        code(initial_dep.config.try_approve_schema_version(&0)),
    );
    observed.record(
        "protocol-config deprecate_schema_version(0)",
        code(initial_dep.config.try_deprecate_schema_version(&0)),
    );
    let mut schema_batch = soroban_sdk::Vec::new(env);
    for version in 0..=earnproof_shared::MAX_SCHEMA_STATUS_BATCH {
        schema_batch.push_back(version);
    }
    observed.record(
        "protocol-config oversized schema status batch",
        code(initial_dep.config.try_get_schema_statuses(&schema_batch)),
    );

    // --- issuer-registry -------------------------------------------------
    observed.record(
        "issuer-registry initialize twice",
        code(initial_dep.issuers.try_initialize(&initial_dep.admin)),
    );
    observed.record(
        "issuer-registry duplicate issuer id",
        code(initial_dep.issuers.try_register_issuer(
            &bytes32(env, 1),
            &Address::generate(env),
            &bytes32(env, 3),
            &bytes32(env, 99),
        )),
    );
    observed.record(
        "issuer-registry duplicate issuer address",
        code(initial_dep.issuers.try_register_issuer(
            &bytes32(env, 9),
            &initial_dep.issuer,
            &bytes32(env, 3),
            &bytes32(env, 99),
        )),
    );
    observed.record(
        "issuer-registry set_issuer_metadata_commitment with all-zero digest",
        code(initial_dep.issuers.try_set_issuer_metadata_commitment(
            &bytes32(env, 1),
            &soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
            &bytes32(env, 3),
        )),
    );
    observed.record(
        "issuer-registry update unknown issuer",
        code(
            initial_dep
                .issuers
                .try_update_issuer(&bytes32(env, 99), &bytes32(env, 3)),
        ),
    );
    observed.record(
        "issuer-registry get unknown issuer",
        code(initial_dep.issuers.try_get_issuer(&bytes32(env, 99))),
    );
    observed.record(
        "issuer-registry lookup unknown address",
        code(
            initial_dep
                .issuers
                .try_get_issuer_by_address(&Address::generate(env)),
        ),
    );
    let mut issuer_batch = soroban_sdk::Vec::new(env);
    for index in 0..=earnproof_shared::MAX_ISSUER_STATUS_BATCH {
        issuer_batch.push_back(bytes32(env, index as u8));
    }
    observed.record(
        "issuer-registry oversized status batch",
        code(initial_dep.issuers.try_get_issuer_statuses(&issuer_batch)),
    );
    observed.record(
        "issuer-registry invalid metadata commitment",
        code(initial_dep.issuers.try_set_issuer_metadata_commitment(
            &bytes32(env, 1),
            &bytes32(env, 0),
            &bytes32(env, 99),
        )),
    );

    let revoked_issuer = Address::generate(env);
    initial_dep.issuers.register_issuer(
        &bytes32(env, 20),
        &revoked_issuer,
        &bytes32(env, 21),
        &bytes32(env, 99),
    );
    initial_dep.issuers.revoke_issuer(
        &bytes32(env, 20),
        &soroban_sdk::BytesN::from_array(env, &[1u8; 32]),
    );
    observed.record(
        "issuer-registry update revoked issuer",
        code(
            initial_dep
                .issuers
                .try_update_issuer(&bytes32(env, 20), &bytes32(env, 22)),
        ),
    );
    observed.record(
        "issuer-registry reactivate revoked issuer",
        code(initial_dep.issuers.try_reactivate_issuer(
            &bytes32(env, 20),
            &soroban_sdk::BytesN::from_array(env, &[1u8; 32]),
        )),
    );

    // --- proof-registry --------------------------------------------------
    let fresh_proofs = env.register(ProofRegistryContract, ());
    let fresh_proofs = ProofRegistryContractClient::new(env, &fresh_proofs);
    observed.record(
        "proof-registry get_admin uninitialized",
        code(fresh_proofs.try_get_admin()),
    );
    observed.record(
        "proof-registry initialize twice",
        code(initial_dep.proofs.try_initialize(
            &initial_dep.admin,
            &initial_dep.issuers_id,
            &initial_dep.config_id,
        )),
    );

    let proof_id = bytes32(env, 5);
    initial_dep.proofs.register_proof_with_type_identifier(
        &proof_id,
        &bytes32(env, 6),
        &initial_dep.issuer,
        &1,
        &FAR_FUTURE,
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );
    observed.record(
        "proof-registry rejects a protocol-config address as issuer",
        code(initial_dep.proofs.try_register_proof_with_type_identifier(
            &bytes32(env, 0xA5),
            &bytes32(env, 0xA6),
            &initial_dep.config_id,
            &1,
            &FAR_FUTURE,
            &soroban_sdk::BytesN::from_array(env, &[1u8; 32]),
        )),
    );
    observed.record(
        "proof-registry duplicate proof id",
        code(initial_dep.proofs.try_register_proof_with_type_identifier(
            &proof_id,
            &bytes32(env, 7),
            &initial_dep.issuer,
            &1,
            &FAR_FUTURE,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        )),
    );
    observed.record(
        "proof-registry get unknown proof",
        code(initial_dep.proofs.try_get_proof(&bytes32(env, 99))),
    );
    initial_dep.proofs.revoke_proof(&proof_id);
    observed.record(
        "proof-registry revoke twice",
        code(initial_dep.proofs.try_revoke_proof(&proof_id)),
    );
    observed.record(
        "proof-registry expiration in the past",
        code(initial_dep.proofs.try_register_proof_with_type_identifier(
            &bytes32(env, 30),
            &bytes32(env, 31),
            &initial_dep.issuer,
            &1,
            &0,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        )),
    );
    observed.record(
        "proof-registry schema version zero",
        code(initial_dep.proofs.try_register_proof_with_type_identifier(
            &bytes32(env, 32),
            &bytes32(env, 33),
            &initial_dep.issuer,
            &0,
            &FAR_FUTURE,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        )),
    );
    observed.record(
        "proof-registry unapproved schema version",
        code(initial_dep.proofs.try_register_proof_with_type_identifier(
            &bytes32(env, 34),
            &bytes32(env, 35),
            &initial_dep.issuer,
            &7,
            &FAR_FUTURE,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        )),
    );
    observed.record(
        "proof-registry payload exceeds schema limit",
        code(
            initial_dep
                .proofs
                .try_register_proof_with_type_identifier_and_payload(
                    &bytes32(env, 36),
                    &bytes32(env, 37),
                    &initial_dep.issuer,
                    &1,
                    &FAR_FUTURE,
                    &soroban_sdk::BytesN::from_array(env, &[1u8; 32]),
                    &soroban_sdk::Bytes::from_array(
                        env,
                        &[0u8; (earnproof_shared::DEFAULT_SCHEMA_PAYLOAD_LIMIT + 1) as usize],
                    ),
                ),
        ),
    );
    // New precondition codes (307-309): drive a real failure path for each.
    // 307: ContractPaused — pause the protocol then attempt registration.
    let deployment2 = deployment();
    let env2 = &deployment2.env;
    deployment2.config.pause();
    observed.record(
        "proof-registry contract paused",
        code(deployment2.proofs.try_register_proof_with_type_identifier(
            &bytes32(env2, 40),
            &bytes32(env2, 41),
            &deployment2.issuer,
            &1,
            &FAR_FUTURE,
            &soroban_sdk::BytesN::from_array(&env2, &[1u8; 32]),
        )),
    );

    // 308: IssuerInactive — suspend the issuer then attempt registration.
    let deployment3 = deployment();
    let env3 = &deployment3.env;
    deployment3.issuers.suspend_issuer(
        &bytes32(env3, 1),
        &soroban_sdk::BytesN::from_array(env3, &[1u8; 32]),
    );
    observed.record(
        "proof-registry issuer inactive",
        code(deployment3.proofs.try_register_proof_with_type_identifier(
            &bytes32(env3, 50),
            &bytes32(env3, 51),
            &deployment3.issuer,
            &1,
            &FAR_FUTURE,
            &soroban_sdk::BytesN::from_array(&env3, &[1u8; 32]),
        )),
    );

    // 309: UnsupportedSchema — use an unapproved schema version on a live contract.
    let deployment4 = deployment();
    let env4 = &deployment4.env;
    observed.record(
        "proof-registry unsupported schema",
        code(deployment4.proofs.try_register_proof_with_type_identifier(
            &bytes32(env4, 60),
            &bytes32(env4, 61),
            &deployment4.issuer,
            &7,
            &FAR_FUTURE,
            &soroban_sdk::BytesN::from_array(&env4, &[1u8; 32]),
        )),
    );

    // Supersession errors use the appended proof-registry code range.
    let deployment_cyclic = deployment();
    let env_cyclic = &deployment_cyclic.env;
    observed.record(
        "proof-registry cyclic supersession",
        code(deployment_cyclic.proofs.try_register_proof_with_predecessor(
            &bytes32(env_cyclic, 70),
            &bytes32(env_cyclic, 71),
            &deployment_cyclic.issuer,
            &1,
            &FAR_FUTURE,
            &Some(bytes32(env_cyclic, 70)),
            &soroban_sdk::BytesN::from_array(env_cyclic, &[1; 32]),
        )),
    );

    let deployment_cross = deployment();
    let env_cross = &deployment_cross.env;
    let issuer_cross = Address::generate(env_cross);
    deployment_cross.issuers.register_issuer(
        &bytes32(env_cross, 80),
        &issuer_cross,
        &bytes32(env_cross, 81),
        &bytes32(env_cross, 82),
    );
    deployment_cross.proofs.register_proof_with_type_identifier(
        &bytes32(env_cross, 83),
        &bytes32(env_cross, 84),
        &deployment_cross.issuer,
        &1,
        &FAR_FUTURE,
        &soroban_sdk::BytesN::from_array(env_cross, &[1; 32]),
    );
    observed.record(
        "proof-registry cross issuer supersession",
        code(deployment_cross.proofs.try_register_proof_with_predecessor(
            &bytes32(env_cross, 85),
            &bytes32(env_cross, 86),
            &issuer_cross,
            &1,
            &FAR_FUTURE,
            &Some(bytes32(env_cross, 83)),
            &soroban_sdk::BytesN::from_array(env_cross, &[1; 32]),
        )),
    );

    let deployment_missing = deployment();
    let env_missing = &deployment_missing.env;
    observed.record(
        "proof-registry predecessor not found",
        code(deployment_missing.proofs.try_register_proof_with_predecessor(
            &bytes32(env_missing, 90),
            &bytes32(env_missing, 91),
            &deployment_missing.issuer,
            &1,
            &FAR_FUTURE,
            &Some(bytes32(env_missing, 92)),
            &soroban_sdk::BytesN::from_array(env_missing, &[1; 32]),
        )),
    );

    let deployment_many = deployment();
    let env_many = &deployment_many.env;
    deployment_many.proofs.register_proof_with_type_identifier(
        &bytes32(env_many, 100),
        &bytes32(env_many, 101),
        &deployment_many.issuer,
        &1,
        &FAR_FUTURE,
        &soroban_sdk::BytesN::from_array(env_many, &[1; 32]),
    );
    for i in 1..=earnproof_shared::MAX_SUCCESSORS {
        deployment_many.proofs.register_proof_with_predecessor(
            &bytes32(env_many, (100 + i) as u8),
            &bytes32(env_many, (200 + i) as u8),
            &deployment_many.issuer,
            &1,
            &FAR_FUTURE,
            &Some(bytes32(env_many, 100)),
            &soroban_sdk::BytesN::from_array(env_many, &[1; 32]),
        );
    }
    observed.record(
        "proof-registry too many successors",
        code(deployment_many.proofs.try_register_proof_with_predecessor(
            &bytes32(env_many, 110),
            &bytes32(env_many, 111),
            &deployment_many.issuer,
            &1,
            &FAR_FUTURE,
            &Some(bytes32(env_many, 100)),
            &soroban_sdk::BytesN::from_array(env_many, &[1; 32]),
        )),
    );

    let deployment5 = deployment();
    let env5 = &deployment5.env;
    observed.record(
        "proof-registry unsupported proof type",
        code(deployment5.proofs.try_register_proof_with_type_identifier(
            &bytes32(env5, 70),
            &bytes32(env5, 71),
            &deployment5.issuer,
            &1,
            &FAR_FUTURE,
            &bytes32(env5, 2),
        )),
    );

    // --- issuer-registry capacity and cooldown --------------------------
    // A dedicated registry keeps the active-count accounting isolated from the
    // paths above.
    let cap_id = env.register(IssuerRegistryContract, ());
    let cap = IssuerRegistryContractClient::new(env, &cap_id);
    cap.initialize(&initial_dep.admin);
    let cap_issuer = Address::generate(env);
    cap.register_issuer(
        &bytes32(env, 50),
        &cap_issuer,
        &bytes32(env, 51),
        &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]),
    );

    observed.record(
        "issuer-registry set_max below active usage",
        code(cap.try_set_max_active_issuers(&0, &false)),
    );

    cap.set_max_active_issuers(&1, &false);
    observed.record(
        "issuer-registry register beyond capacity",
        code(cap.try_register_issuer(
            &bytes32(env, 52),
            &Address::generate(env),
            &bytes32(env, 53),
            &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]),
        )),
    );

    cap.set_reactivation_cooldown(&1_000);
    cap.suspend_issuer(
        &bytes32(env, 50),
        &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]),
    );
    observed.record(
        "issuer-registry reactivate before cooldown",
        code(cap.try_reactivate_issuer(
            &bytes32(env, 50),
            &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]),
        )),
    );

    // --- proof-registry incompatible dependency -------------------------
    let bad_registry = env.register(BadVersionRegistry, ());
    observed.record(
        "proof-registry bind incompatible issuer registry",
        code(initial_dep.proofs.try_set_issuer_registry(&bad_registry)),
    );

    // 311: InvalidBatchSize — an empty batch is rejected before any
    // cross-contract call is made, on both the registration and revocation
    // paths.
    let empty_registration_batch: soroban_sdk::Vec<earnproof_shared::ProofRegistrationInput> =
        soroban_sdk::Vec::new(env);
    observed.record(
        "proof-registry registration batch with zero entries",
        code(
            initial_dep
                .proofs
                .try_register_proofs_batch(&empty_registration_batch, &initial_dep.issuer),
        ),
    );

    let empty_revocation_batch: soroban_sdk::Vec<soroban_sdk::BytesN<32>> =
        soroban_sdk::Vec::new(env);
    observed.record(
        "proof-registry revocation batch with zero entries",
        code(
            initial_dep
                .proofs
                .try_revoke_proofs_batch(&empty_revocation_batch),
        ),
    );

    // 312: InvalidActivationTime — activation at or after expiry can never
    // be valid.
    observed.record(
        "proof-registry activation at or after expiry",
        code(initial_dep.proofs.try_register_proof_with_activation(
            &bytes32(env, 70),
            &bytes32(env, 71),
            &initial_dep.issuer,
            &1,
            &FAR_FUTURE,
            &soroban_sdk::BytesN::from_array(env, &[1u8; 32]),
            &FAR_FUTURE,
        )),
    );

    // 313: DisputeAlreadyOpen — opening a second dispute while one is open.
    initial_dep
        .proofs
        .open_dispute(&proof_id, &initial_dep.issuer, &bytes32(env, 80));
    observed.record(
        "proof-registry dispute already open",
        code(initial_dep.proofs.try_open_dispute(
            &proof_id,
            &initial_dep.issuer,
            &bytes32(env, 81),
        )),
    );

    // 314: DisputeNotFound — no dispute exists for this proof.
    observed.record(
        "proof-registry dispute not found",
        code(initial_dep.proofs.try_withdraw_dispute(&bytes32(env, 99))),
    );

    // 315: DisputeNotOpen — the dispute above is withdrawn, then acted on again.
    initial_dep.proofs.withdraw_dispute(&proof_id);
    observed.record(
        "proof-registry dispute not open",
        code(initial_dep.proofs.try_withdraw_dispute(&proof_id)),
    );

    // --- issuer-registry capacity and cooldown --------------------------
    // A dedicated registry keeps the active-count accounting isolated from the
    // paths above.
    let cap_id = env.register(IssuerRegistryContract, ());
    let cap = IssuerRegistryContractClient::new(env, &cap_id);
    cap.initialize(&initial_dep.admin);
    let cap_issuer = Address::generate(env);
    cap.register_issuer(
        &bytes32(env, 50),
        &cap_issuer,
        &bytes32(env, 51),
        &bytes32(env, 59),
    );

    observed.record(
        "issuer-registry set_max below active usage",
        code(cap.try_set_max_active_issuers(&0, &false)),
    );

    cap.set_max_active_issuers(&1, &false);
    observed.record(
        "issuer-registry register beyond capacity",
        code(cap.try_register_issuer(
            &bytes32(env, 52),
            &Address::generate(env),
            &bytes32(env, 53),
            &bytes32(env, 58),
        )),
    );

    cap.set_reactivation_cooldown(&1_000);
    cap.suspend_issuer(&bytes32(env, 50), &bytes32(env, 57));
    observed.record(
        "issuer-registry reactivate before cooldown",
        code(cap.try_reactivate_issuer(&bytes32(env, 50), &bytes32(env, 56))),
    );

    observed.record(
        "issuer-registry set metadata commitment empty",
        code(cap.try_set_issuer_metadata_commitment(
            &bytes32(env, 50),
            &soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
            &bytes32(env, 53),
        )),
    );

    // --- proof-registry incompatible dependency -------------------------
    let bad_registry = env.register(BadVersionRegistry, ());
    observed.record(
        "proof-registry bind incompatible issuer registry",
        code(initial_dep.proofs.try_propose_dependency_replacement(
            &bytes32(env, 0x13),
            &bad_registry,
            &initial_dep.proofs.get_protocol_config(),
            &(env.ledger().sequence() + 10),
        )),
    );

    // Every catalogued `Returned` code must appear at least once above.
    for entry in ERROR_CATALOG {
        if entry.status == Status::Returned {
            assert!(
                observed.codes.contains(&entry.code),
                "{} ({}) is catalogued as returned but no failure path here produces it",
                entry.name,
                entry.code
            );
        } else {
            assert!(
                !observed.codes.contains(&entry.code),
                "{} ({}) is catalogued as reserved but a failure path produced it",
                entry.name,
                entry.code
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Individually named paths for the codes a backend is most likely to branch on
// ---------------------------------------------------------------------------

#[test]
fn a_paused_protocol_is_reported_as_contract_paused() {
    // Distinct code introduced by issue #136. Asserting it here means the
    // documentation stays honest, and a future change that alters the pause
    // code has to update the catalog in the same change.
    let deployment = deployment();
    deployment.config.pause();

    let result = deployment.proofs.try_register_proof_with_type_identifier(
        &bytes32(&deployment.env, 1),
        &bytes32(&deployment.env, 2),
        &deployment.issuer,
        &1,
        &FAR_FUTURE,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );

    assert_eq!(result, Err(Ok(ProofError::ContractPaused)));
    assert_ne!(
        ProofError::ContractPaused as u32,
        ContractError::ProtocolPaused as u32,
        "proof-registry ContractPaused must not collide with the common ProtocolPaused code"
    );
}

#[test]
fn a_suspended_issuer_is_reported_as_issuer_inactive() {
    let deployment = deployment();
    let env = &deployment.env;
    let suspended = Address::generate(env);
    deployment.issuers.register_issuer(
        &bytes32(env, 40),
        &suspended,
        &bytes32(env, 41),
        &bytes32(env, 99),
    );
    deployment.issuers.suspend_issuer(
        &bytes32(env, 40),
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );

    let result = deployment.proofs.try_register_proof_with_type_identifier(
        &bytes32(env, 42),
        &bytes32(env, 43),
        &suspended,
        &1,
        &FAR_FUTURE,
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );

    assert_eq!(result, Err(Ok(ProofError::IssuerInactive)));
    assert_ne!(
        ProofError::IssuerInactive as u32,
        IssuerError::IssuerInactive as u32,
        "proof-registry IssuerInactive must not collide with issuer-registry IssuerInactive"
    );
}

#[test]
fn an_uninitialized_proof_registry_reports_proof_not_found_and_writes_nothing() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(ProofRegistryContract, ());
    let proofs = ProofRegistryContractClient::new(&env, &contract);
    let issuer = Address::generate(&env);

    let result = proofs.try_register_proof_with_type_identifier(
        &bytes32(&env, 1),
        &bytes32(&env, 2),
        &issuer,
        &1,
        &FAR_FUTURE,
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );

    assert_eq!(result, Err(Ok(ProofError::ProofNotFound)));
    assert!(!proofs.is_valid_proof(&bytes32(&env, 1)));
}

#[test]
fn a_registry_pointed_at_an_empty_config_reports_unsupported_schema() {
    let deployment = deployment();
    let env = &deployment.env;
    let empty_config = env.register(ProtocolConfigContract, ());
    let proofs_id = env.register(ProofRegistryContract, ());
    let proofs = ProofRegistryContractClient::new(env, &proofs_id);
    proofs.initialize(&deployment.admin, &deployment.issuers_id, &empty_config);

    let result = proofs.try_register_proof_with_type_identifier(
        &bytes32(env, 1),
        &bytes32(env, 2),
        &deployment.issuer,
        &1,
        &FAR_FUTURE,
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );

    assert_eq!(result, Err(Ok(ProofError::UnsupportedSchema)));
}

#[test]
fn a_returned_error_carries_a_code_and_nothing_else() {
    // Soroban contract errors are a type and a number. There is no message, no
    // payload, and therefore nothing for a failing call to disclose about the
    // record it touched. This is the structural reason the catalog can promise
    // that authorization and lookup failures reveal no protected state.
    let deployment = deployment();
    let unknown = bytes32(&deployment.env, 99);

    let error = deployment
        .proofs
        .try_get_proof(&unknown)
        .expect_err("unknown proof must fail")
        .expect("must be a contract error rather than a host error");

    assert_eq!(error, ProofError::ProofNotFound);
    assert_eq!(error as u32, 301);
}

/// Extracts the contract error code from a `try_` result, failing the test if
/// the call succeeded or aborted with a host error.
fn code<T, E>(result: Result<T, Result<E, soroban_sdk::InvokeError>>) -> u32
where
    E: Into<soroban_sdk::Error> + Clone,
    T: core::fmt::Debug,
{
    match result {
        Ok(value) => std::panic!("expected a failure, got {value:?}"),
        Err(Ok(error)) => {
            let error: soroban_sdk::Error = error.into();
            error.get_code()
        }
        Err(Err(invoke)) => std::panic!("expected a contract error, got {invoke:?}"),
    }
}
