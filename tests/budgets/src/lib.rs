//! Resource budget regression tests for EarnProof contracts.
//!
//! These tests measure CPU instructions, memory usage, and ledger I/O for
//! representative worst-case operations to detect performance regressions.
//!
//! Thresholds include 20% headroom above current measurements and will fail
//! CI if exceeded, forcing explicit review of resource usage changes.
//!
//! To update baselines after intentional optimizations or feature additions:
//! 1. Review the resource usage changes in the test output
//! 2. Verify changes are justified and documented
//! 3. Update threshold constants in this file
//! 4. Document the change in the PR description

#[cfg(test)]
mod tests {
    use earnproof_shared::{MAX_ISSUER_STATUS_BATCH, MAX_MIGRATION_BATCH, MAX_SCHEMA_STATUS_BATCH};
    use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
    use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
    use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
    use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env, Vec};

    // -----------------------------------------------------------------------
    // Threshold Constants
    //
    // These values represent maximum acceptable resource usage for each
    // operation. Values include ~20% headroom above baseline measurements.
    //
    // CPU instructions measured on:
    // - Soroban SDK v27.0.0
    // - Rust stable from rust-toolchain.toml
    // - x86_64-unknown-linux-gnu
    //
    // Update these values when:
    // - Adding new contract functionality
    // - Optimizing existing operations
    // - Upgrading Soroban SDK version
    // -----------------------------------------------------------------------

    // Protocol Config thresholds
    const PROTOCOL_INIT_CPU_MAX: u64 = 300_000;
    const PROTOCOL_INIT_MEM_MAX: u64 = 100_000;
    // Includes the two fixed-size incident metadata writes added to pause,
    // plus one bounded change-history ring append (issue #193).
    const PROTOCOL_PAUSE_CPU_MAX: u64 = 300_000;
    const PROTOCOL_PAUSE_MEM_MAX: u64 = 90_000;
    // Includes one bounded change-history ring append (issue #193).
    const PROTOCOL_MIGRATION_STEP_CPU_MAX: u64 = 250_000;
    const PROTOCOL_MIGRATION_STEP_MEM_MAX: u64 = 80_000;
    // Worst-case bounded batch schema status query: a full
    // MAX_SCHEMA_STATUS_BATCH of approved versions, each requiring a persistent
    // read. Thresholds include ~20% headroom over the measured baseline.
    const PROTOCOL_SCHEMA_BATCH_STATUS_CPU_MAX: u64 = 2_200_000;
    const PROTOCOL_SCHEMA_BATCH_STATUS_MEM_MAX: u64 = 700_000;
    // Includes one bounded change-history ring append (issue #193).
    const PROTOCOL_SCHEMA_APPROVE_CPU_MAX: u64 = 335_000;
    const PROTOCOL_SCHEMA_APPROVE_MEM_MAX: u64 = 100_000;

    // Issuer Registry thresholds
    const ISSUER_INIT_CPU_MAX: u64 = 300_000;
    const ISSUER_INIT_MEM_MAX: u64 = 100_000;
    const ISSUER_REGISTER_CPU_MAX: u64 = 600_000;
    const ISSUER_REGISTER_MEM_MAX: u64 = 200_000;
    const ISSUER_LOOKUP_CPU_MAX: u64 = 210_000;
    const ISSUER_LOOKUP_MEM_MAX: u64 = 100_000;
    const ISSUER_UPDATE_CPU_MAX: u64 = 400_000;
    const ISSUER_UPDATE_MEM_MAX: u64 = 150_000;
    const ISSUER_SUSPEND_CPU_MAX: u64 = 560_000;
    const ISSUER_SUSPEND_MEM_MAX: u64 = 160_000;
    const ISSUER_REVOKE_CPU_MAX: u64 = 500_000;
    const ISSUER_REVOKE_MEM_MAX: u64 = 150_000;
    const ISSUER_ROTATE_CPU_MAX: u64 = 500_000;
    const ISSUER_ROTATE_MEM_MAX: u64 = 180_000;
    // Worst-case bounded batch status query: a full MAX_ISSUER_STATUS_BATCH of
    // registered identifiers, each requiring a persistent read and a TTL
    // extension. Thresholds include ~20% headroom over the measured baseline.
    const ISSUER_BATCH_STATUS_CPU_MAX: u64 = 4_200_000;
    const ISSUER_BATCH_STATUS_MEM_MAX: u64 = 1_500_000;

    // Proof Registry thresholds
    const PROOF_INIT_CPU_MAX: u64 = 400_000;
    const PROOF_INIT_MEM_MAX: u64 = 120_000;
    const PROOF_REGISTER_CPU_MAX: u64 = 1_050_000;
    const PROOF_REGISTER_MEM_MAX: u64 = 400_000;
    const PROOF_LOOKUP_CPU_MAX: u64 = 200_000;
    const PROOF_LOOKUP_MEM_MAX: u64 = 100_000;
    const PROOF_REVOKE_CPU_MAX: u64 = 550_000;
    const PROOF_REVOKE_MEM_MAX: u64 = 200_000;
    const PROOF_VALIDITY_CHECK_CPU_MAX: u64 = 200_000;
    const PROOF_VALIDITY_CHECK_MEM_MAX: u64 = 100_000;
    const PROOF_REGISTER_BATCH_MAX_CPU_MAX: u64 = 5_600_000;
    const PROOF_REGISTER_BATCH_MAX_MEM_MAX: u64 = 1_750_000;
    const PROOF_REGISTER_WITH_ACTIVATION_CPU_MAX: u64 = 800_000;
    const PROOF_REGISTER_WITH_ACTIVATION_MEM_MAX: u64 = 320_000;
    const PROOF_REVOKE_BATCH_MAX_CPU_MAX: u64 = 6_800_000;
    const PROOF_REVOKE_BATCH_MAX_MEM_MAX: u64 = 2_000_000;
    const PROOF_OPEN_DISPUTE_CPU_MAX: u64 = 500_000;
    const PROOF_OPEN_DISPUTE_MEM_MAX: u64 = 180_000;
    const PROOF_RESOLVE_DISPUTE_CPU_MAX: u64 = 500_000;
    const PROOF_RESOLVE_DISPUTE_MEM_MAX: u64 = 190_000;

    // -----------------------------------------------------------------------
    // Test Utilities
    // -----------------------------------------------------------------------

    const ADMIN: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";
    const ISSUER_ONE: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";
    const ISSUER_TWO: &str = "GDWUSKGGFDI4FRXK5EBTRECZSVQSSWJHHJOGH6JWG3AUMFFMQ435DIAG";

    fn bytes(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn assert_budget(env: &Env, operation: &str, cpu_max: u64, mem_max: u64) {
        let budget = env.cost_estimate().budget();
        let cpu_used = budget.cpu_instruction_cost();
        let mem_used = budget.memory_bytes_cost();

        println!(
            "{}: CPU={} (max={}), Memory={} (max={})",
            operation, cpu_used, cpu_max, mem_used, mem_max
        );

        assert!(
            cpu_used <= cpu_max,
            "CPU regression detected for {}: {} > {} (+{}%)",
            operation,
            cpu_used,
            cpu_max,
            ((cpu_used as f64 / cpu_max as f64 - 1.0) * 100.0) as i64
        );

        assert!(
            mem_used <= mem_max,
            "Memory regression detected for {}: {} > {} (+{}%)",
            operation,
            mem_used,
            mem_max,
            ((mem_used as f64 / mem_max as f64 - 1.0) * 100.0) as i64
        );
    }

    // -----------------------------------------------------------------------
    // Protocol Config Budget Tests
    // -----------------------------------------------------------------------

    #[test]
    fn protocol_config_initialize_budget() {
        let env = Env::default();
        env.mock_all_auths();
        env.cost_estimate().budget().reset_unlimited();

        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        client.initialize(&admin);

        assert_budget(
            &env,
            "protocol_config.initialize",
            PROTOCOL_INIT_CPU_MAX,
            PROTOCOL_INIT_MEM_MAX,
        );
    }

    #[test]
    fn protocol_config_pause_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        client.initialize(&admin);
        env.cost_estimate().budget().reset_unlimited();

        client.pause();

        assert_budget(
            &env,
            "protocol_config.pause",
            PROTOCOL_PAUSE_CPU_MAX,
            PROTOCOL_PAUSE_MEM_MAX,
        );
    }

    #[test]
    fn protocol_config_approve_schema_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        client.initialize(&admin);
        env.cost_estimate().budget().reset_unlimited();

        client.approve_schema_version(&1);

        assert_budget(
            &env,
            "protocol_config.approve_schema_version",
            PROTOCOL_SCHEMA_APPROVE_CPU_MAX,
            PROTOCOL_SCHEMA_APPROVE_MEM_MAX,
        );
    }

    #[test]
    fn protocol_config_max_migration_batch_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        client.initialize(&admin);
        client.begin_migration(&2, &MAX_MIGRATION_BATCH);
        env.cost_estimate().budget().reset_unlimited();

        client.advance_migration(&0, &MAX_MIGRATION_BATCH);

        assert_budget(
            &env,
            "protocol_config.advance_migration(max_batch)",
            PROTOCOL_MIGRATION_STEP_CPU_MAX,
            PROTOCOL_MIGRATION_STEP_MEM_MAX,
        );
    }

    #[test]
    fn protocol_config_batch_schema_status_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        client.initialize(&admin);

        // Approve the maximum number of versions so the worst-case batch reads a
        // real record for every entry.
        let mut request: Vec<u32> = Vec::new(&env);
        for version in 1..=MAX_SCHEMA_STATUS_BATCH {
            client.approve_schema_version(&version);
            request.push_back(version);
        }

        env.cost_estimate().budget().reset_unlimited();

        let results = client.get_schema_statuses(&request);
        assert_eq!(results.len(), MAX_SCHEMA_STATUS_BATCH);

        assert_budget(
            &env,
            "protocol_config.get_schema_statuses",
            PROTOCOL_SCHEMA_BATCH_STATUS_CPU_MAX,
            PROTOCOL_SCHEMA_BATCH_STATUS_MEM_MAX,
        );
    }

    #[test]
    fn protocol_config_batch_schema_status_edge_cases_within_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        client.initialize(&admin);

        client.approve_schema_version(&1);
        client.approve_schema_version(&2);
        client.deprecate_schema_version(&2);

        env.cost_estimate().budget().reset_unlimited();

        // Empty, then a mix of duplicate, deprecated, and unknown versions —
        // all comfortably inside the worst-case budget.
        let empty: Vec<u32> = Vec::new(&env);
        assert_eq!(client.get_schema_statuses(&empty).len(), 0);

        let mixed = vec![&env, 1u32, 1u32, 2u32, 99u32, 0u32];
        let results = client.get_schema_statuses(&mixed);
        assert_eq!(results.len(), 5);

        assert_budget(
            &env,
            "protocol_config.get_schema_statuses.edge_cases",
            PROTOCOL_SCHEMA_BATCH_STATUS_CPU_MAX,
            PROTOCOL_SCHEMA_BATCH_STATUS_MEM_MAX,
        );
    }

    // -----------------------------------------------------------------------
    // Issuer Registry Budget Tests
    // -----------------------------------------------------------------------

    #[test]
    fn issuer_registry_initialize_budget() {
        let env = Env::default();
        env.mock_all_auths();
        env.cost_estimate().budget().reset_unlimited();

        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        client.initialize(&admin);

        assert_budget(
            &env,
            "issuer_registry.initialize",
            ISSUER_INIT_CPU_MAX,
            ISSUER_INIT_MEM_MAX,
        );
    }

    #[test]
    fn issuer_registry_register_issuer_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        client.initialize(&admin);
        env.cost_estimate().budget().reset_unlimited();

        let issuer_id = bytes(&env, 1);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);
        let metadata_hash = bytes(&env, 2);

        client.register_issuer(&issuer_id, &issuer_address, &metadata_hash, &metadata_hash);

        assert_budget(
            &env,
            "issuer_registry.register_issuer",
            ISSUER_REGISTER_CPU_MAX,
            ISSUER_REGISTER_MEM_MAX,
        );
    }

    #[test]
    fn issuer_registry_get_issuer_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let issuer_id = bytes(&env, 1);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);
        let metadata_hash = bytes(&env, 2);

        client.initialize(&admin);
        client.register_issuer(&issuer_id, &issuer_address, &metadata_hash, &metadata_hash);
        env.cost_estimate().budget().reset_unlimited();

        client.get_issuer(&issuer_id);

        assert_budget(
            &env,
            "issuer_registry.get_issuer",
            ISSUER_LOOKUP_CPU_MAX,
            ISSUER_LOOKUP_MEM_MAX,
        );
    }

    #[test]
    fn issuer_registry_update_issuer_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let issuer_id = bytes(&env, 1);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);
        let metadata_hash = bytes(&env, 2);

        client.initialize(&admin);
        client.register_issuer(&issuer_id, &issuer_address, &metadata_hash, &metadata_hash);
        env.cost_estimate().budget().reset_unlimited();

        let new_metadata = bytes(&env, 99);
        client.update_issuer(&issuer_id, &new_metadata);

        assert_budget(
            &env,
            "issuer_registry.update_issuer",
            ISSUER_UPDATE_CPU_MAX,
            ISSUER_UPDATE_MEM_MAX,
        );
    }

    #[test]
    fn issuer_registry_suspend_issuer_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let issuer_id = bytes(&env, 1);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);
        let metadata_hash = bytes(&env, 2);

        client.initialize(&admin);
        client.register_issuer(&issuer_id, &issuer_address, &metadata_hash, &metadata_hash);
        env.cost_estimate().budget().reset_unlimited();

        client.suspend_issuer(
            &issuer_id,
            &soroban_sdk::BytesN::from_array(&client.env, &[1u8; 32]),
        );

        assert_budget(
            &env,
            "issuer_registry.suspend_issuer",
            ISSUER_SUSPEND_CPU_MAX,
            ISSUER_SUSPEND_MEM_MAX,
        );
    }

    #[test]
    fn issuer_registry_revoke_issuer_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let issuer_id = bytes(&env, 1);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);
        let metadata_hash = bytes(&env, 2);

        client.initialize(&admin);
        client.register_issuer(&issuer_id, &issuer_address, &metadata_hash, &metadata_hash);
        env.cost_estimate().budget().reset_unlimited();

        client.revoke_issuer(
            &issuer_id,
            &soroban_sdk::BytesN::from_array(&client.env, &[1u8; 32]),
        );

        assert_budget(
            &env,
            "issuer_registry.revoke_issuer",
            ISSUER_REVOKE_CPU_MAX,
            ISSUER_REVOKE_MEM_MAX,
        );
    }

    #[test]
    fn issuer_registry_rotate_address_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let issuer_id = bytes(&env, 1);
        let old_address = Address::from_str(&env, ISSUER_ONE);
        let new_address = Address::from_str(&env, ISSUER_TWO);
        let metadata_hash = bytes(&env, 2);

        client.initialize(&admin);
        client.register_issuer(&issuer_id, &old_address, &metadata_hash, &metadata_hash);
        env.cost_estimate().budget().reset_unlimited();

        client.rotate_issuer_address(&issuer_id, &new_address);

        assert_budget(
            &env,
            "issuer_registry.rotate_issuer_address",
            ISSUER_ROTATE_CPU_MAX,
            ISSUER_ROTATE_MEM_MAX,
        );
    }

    #[test]
    fn issuer_registry_batch_status_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        client.initialize(&admin);

        // Register the maximum number of issuers so the worst-case batch reads a
        // real record and extends a TTL for every entry.
        let mut request: Vec<BytesN<32>> = Vec::new(&env);
        for index in 0..MAX_ISSUER_STATUS_BATCH {
            let issuer_id = bytes(&env, index as u8);
            let issuer_address = Address::generate(&env);
            client.register_issuer(
                &issuer_id,
                &issuer_address,
                &bytes(&env, 200),
                &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]),
            );
            request.push_back(issuer_id);
        }

        env.cost_estimate().budget().reset_unlimited();

        let results = client.get_issuer_statuses(&request);
        assert_eq!(results.len(), MAX_ISSUER_STATUS_BATCH);

        assert_budget(
            &env,
            "issuer_registry.get_issuer_statuses",
            ISSUER_BATCH_STATUS_CPU_MAX,
            ISSUER_BATCH_STATUS_MEM_MAX,
        );
    }

    #[test]
    fn issuer_registry_batch_status_edge_cases_within_budget() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        client.initialize(&admin);

        let known = bytes(&env, 1);
        client.register_issuer(
            &known,
            &Address::generate(&env),
            &bytes(&env, 2),
            &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]),
        );
        let missing = bytes(&env, 99);

        env.cost_estimate().budget().reset_unlimited();

        // Minimum (single item), duplicates, and a missing identifier in one
        // ordered request — the smallest meaningful batch stays well inside the
        // worst-case budget.
        let request = vec![&env, known.clone(), known.clone(), missing.clone()];
        let results = client.get_issuer_statuses(&request);
        assert_eq!(results.len(), 3);

        assert_budget(
            &env,
            "issuer_registry.get_issuer_statuses.edge_cases",
            ISSUER_BATCH_STATUS_CPU_MAX,
            ISSUER_BATCH_STATUS_MEM_MAX,
        );
    }

    // -----------------------------------------------------------------------
    // Proof Registry Budget Tests
    // -----------------------------------------------------------------------

    fn setup_proof_registry(
        env: &Env,
    ) -> (
        ProofRegistryContractClient<'_>,
        ProtocolConfigContractClient<'_>,
        IssuerRegistryContractClient<'_>,
        Address,
    ) {
        env.mock_all_auths();

        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let protocol_client = ProtocolConfigContractClient::new(env, &protocol_config_id);

        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let issuer_client = IssuerRegistryContractClient::new(env, &issuer_registry_id);

        let proof_contract_id = env.register(ProofRegistryContract, ());
        let proof_client = ProofRegistryContractClient::new(env, &proof_contract_id);

        let admin = Address::from_str(env, ADMIN);
        let issuer = Address::from_str(env, ISSUER_ONE);
        let issuer_id = bytes(env, 9);

        protocol_client.initialize(&admin);
        protocol_client.approve_schema_version(&1);
        protocol_client.approve_proof_type(&soroban_sdk::BytesN::from_array(env, &[1u8; 32]));
        issuer_client.initialize(&admin);
        issuer_client.register_issuer(&issuer_id, &issuer, &bytes(env, 8), &bytes(env, 99));
        proof_client.initialize(&admin, &issuer_registry_id, &protocol_config_id);

        (proof_client, protocol_client, issuer_client, issuer)
    }

    #[test]
    fn proof_registry_initialize_budget() {
        let env = Env::default();
        env.mock_all_auths();

        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let proof_contract_id = env.register(ProofRegistryContract, ());
        let proof_client = ProofRegistryContractClient::new(&env, &proof_contract_id);
        let admin = Address::from_str(&env, ADMIN);

        env.cost_estimate().budget().reset_unlimited();

        proof_client.initialize(&admin, &issuer_registry_id, &protocol_config_id);

        assert_budget(
            &env,
            "proof_registry.initialize",
            PROOF_INIT_CPU_MAX,
            PROOF_INIT_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_register_proof_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        env.cost_estimate().budget().reset_unlimited();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);

        proof_client.register_proof(
            &proof_id,
            &commitment,
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        assert_budget(
            &env,
            "proof_registry.register_proof",
            PROOF_REGISTER_CPU_MAX,
            PROOF_REGISTER_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_get_proof_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        proof_client.register_proof(
            &proof_id,
            &commitment,
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        env.cost_estimate().budget().reset_unlimited();

        proof_client.get_proof(&proof_id);

        assert_budget(
            &env,
            "proof_registry.get_proof",
            PROOF_LOOKUP_CPU_MAX,
            PROOF_LOOKUP_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_revoke_proof_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        proof_client.register_proof(
            &proof_id,
            &commitment,
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        env.cost_estimate().budget().reset_unlimited();

        proof_client.revoke_proof(&proof_id);

        assert_budget(
            &env,
            "proof_registry.revoke_proof",
            PROOF_REVOKE_CPU_MAX,
            PROOF_REVOKE_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_is_valid_proof_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        proof_client.register_proof(
            &proof_id,
            &commitment,
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        env.cost_estimate().budget().reset_unlimited();

        proof_client.is_valid_proof(&proof_id);

        assert_budget(
            &env,
            "proof_registry.is_valid_proof",
            PROOF_VALIDITY_CHECK_CPU_MAX,
            PROOF_VALIDITY_CHECK_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_register_proofs_batch_max_size_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        let mut batch = soroban_sdk::Vec::new(&env);
        for seed in 0..earnproof_shared::MAX_PROOF_BATCH_SIZE as u8 {
            batch.push_back(earnproof_shared::ProofRegistrationInput {
                proof_id_hash: bytes(&env, seed),
                commitment_hash: bytes(&env, seed.wrapping_add(100)),
                schema_version: 1,
                expires_at: 2_000,
                proof_type: bytes(&env, 1),
            });
        }

        env.cost_estimate().budget().reset_unlimited();

        proof_client.register_proofs_batch(&batch, &issuer);

        assert_budget(
            &env,
            "proof_registry.register_proofs_batch(max_size)",
            PROOF_REGISTER_BATCH_MAX_CPU_MAX,
            PROOF_REGISTER_BATCH_MAX_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_register_proof_with_activation_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        env.cost_estimate().budget().reset_unlimited();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);

        proof_client.register_proof_with_activation(
            &proof_id,
            &commitment,
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &500,
        );

        assert_budget(
            &env,
            "proof_registry.register_proof_with_activation",
            PROOF_REGISTER_WITH_ACTIVATION_CPU_MAX,
            PROOF_REGISTER_WITH_ACTIVATION_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_revoke_proofs_batch_max_size_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        let mut batch = soroban_sdk::Vec::new(&env);
        for seed in 0..earnproof_shared::MAX_PROOF_BATCH_SIZE as u8 {
            let proof_id = bytes(&env, seed);
            proof_client.register_proof(
                &proof_id,
                &bytes(&env, seed.wrapping_add(100)),
                &issuer,
                &1,
                &2_000, &soroban_sdk::BytesN::from_array(&env, &[1u8; 32])
            );
            batch.push_back(proof_id);
        }

        env.cost_estimate().budget().reset_unlimited();

        proof_client.revoke_proofs_batch(&batch);

        assert_budget(
            &env,
            "proof_registry.revoke_proofs_batch(max_size)",
            PROOF_REVOKE_BATCH_MAX_CPU_MAX,
            PROOF_REVOKE_BATCH_MAX_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_open_dispute_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        let proof_id = bytes(&env, 1);
        proof_client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000, &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]));

        env.cost_estimate().budget().reset_unlimited();

        proof_client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));

        assert_budget(
            &env,
            "proof_registry.open_dispute",
            PROOF_OPEN_DISPUTE_CPU_MAX,
            PROOF_OPEN_DISPUTE_MEM_MAX,
        );
    }

    #[test]
    fn proof_registry_resolve_dispute_budget() {
        let env = Env::default();
        let (proof_client, _protocol, _issuer_registry, issuer) = setup_proof_registry(&env);

        let proof_id = bytes(&env, 1);
        proof_client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000, &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]));
        proof_client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));

        env.cost_estimate().budget().reset_unlimited();

        proof_client.resolve_dispute(&proof_id);

        assert_budget(
            &env,
            "proof_registry.resolve_dispute",
            PROOF_RESOLVE_DISPUTE_CPU_MAX,
            PROOF_RESOLVE_DISPUTE_MEM_MAX,
        );
    }

    // -----------------------------------------------------------------------
    // Regression Detection Test
    //
    // This test intentionally performs an expensive operation that should
    // exceed budget thresholds to prove the budget gates work correctly.
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "CPU regression detected")]
    fn budget_gate_detects_cpu_regression() {
        let env = Env::default();
        env.mock_all_auths();
        env.cost_estimate().budget().reset_unlimited();

        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        client.initialize(&admin);

        // Assert with an artificially low threshold to trigger failure
        assert_budget(&env, "regression_test", 1, 100_000);
    }

    #[test]
    #[should_panic(expected = "Memory regression detected")]
    fn budget_gate_detects_memory_regression() {
        let env = Env::default();
        env.mock_all_auths();
        env.cost_estimate().budget().reset_unlimited();

        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        client.initialize(&admin);

        // Assert with an artificially low threshold to trigger failure
        assert_budget(&env, "regression_test", 300_000, 1);
    }
}
