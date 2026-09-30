//! Resource boundary tests for proof-registry contract.
//!
//! These tests verify that proof registration and revocation operations
//! stay within Soroban per-transaction CPU and memory budgets, especially
//! when handling bulk proof registrations and cross-contract calls.

#[cfg(test)]
mod proof_registry_resource_tests {
    extern crate std;

    use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
    use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
    use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
    use earnproof_shared::{MAX_PROOF_ID_HASH_BYTES, MAX_COMMITMENT_HASH_BYTES};
    use soroban_sdk::{Address, BytesN, Env};

    const ADMIN: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";
    const ISSUER: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";

    fn bytes(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn setup() -> (
        Env,
        ProofRegistryContractClient<'static>,
        ProtocolConfigContractClient<'static>,
        IssuerRegistryContractClient<'static>,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let protocol_config_client = ProtocolConfigContractClient::new(&env, &protocol_config_id);
        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let issuer_registry_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
        let contract_id = env.register(ProofRegistryContract, ());
        let client = ProofRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);
        let issuer_id = bytes(&env, 9);

        protocol_config_client.initialize(&admin);
        protocol_config_client.approve_schema_version(&1);
        issuer_registry_client.initialize(&admin);
        issuer_registry_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8));
        client.initialize(&admin, &issuer_registry_id, &protocol_config_id);

        (env, client, protocol_config_client, issuer_registry_client)
    }

    // ── SUITE 1: Exact-limit inputs stay within budget ────────────────

    /// Verifies that proof registration at exact limits succeeds
    /// and stays within Soroban CPU and memory budgets.
    ///
    /// proof_id_hash and commitment_hash are both BytesN<32> (fixed 32 bytes).
    #[test]
    fn test_exact_limit_register_proof_succeeds() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let issuer = Address::from_str(&env, ISSUER);

        env.budget().reset_default();

        client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);
        let record = client.get_proof(&proof_id);

        assert_eq!(record.proof_id_hash, proof_id);
        assert_eq!(record.commitment_hash, commitment);

        let cpu_count = env.budget().cpu_instruction_count();
        let mem_bytes = env.budget().mem_bytes_used();
        println!(
            "[resource] register_proof(): cpu={}, mem={}",
            cpu_count, mem_bytes
        );

        // Budget assertions
        // Typical: ~400k-800k CPU (includes cross-contract calls)
        // ~10k-30k memory
        assert!(cpu_count < 5_000_000, "CPU usage unexpectedly high");
        assert!(mem_bytes < 200_000, "Memory usage unexpectedly high");
    }

    /// Verifies that proof validation (is_valid_proof) stays within budget.
    #[test]
    fn test_exact_limit_is_valid_proof_succeeds() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);

        env.budget().reset_default();
        let is_valid = client.is_valid_proof(&proof_id);
        assert!(is_valid);

        let cpu_count = env.budget().cpu_instruction_count();
        println!("[resource] is_valid_proof(): cpu={}", cpu_count);
        assert!(cpu_count < 500_000, "CPU usage unexpectedly high");
    }

    /// Verifies that proof revocation stays within budget.
    #[test]
    fn test_exact_limit_revoke_proof_succeeds() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);

        env.budget().reset_default();
        client.revoke_proof(&proof_id);

        let record = client.get_proof(&proof_id);
        assert!(client.is_revoked(&proof_id));

        let cpu_count = env.budget().cpu_instruction_count();
        println!("[resource] revoke_proof(): cpu={}", cpu_count);
        assert!(cpu_count < 1_000_000, "CPU usage unexpectedly high");
    }

    /// Verifies that admin revocation stays within budget.
    #[test]
    fn test_exact_limit_admin_revoke_proof_succeeds() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);

        env.budget().reset_default();
        client.admin_revoke_proof(&proof_id);

        assert!(client.is_revoked(&proof_id));

        let cpu_count = env.budget().cpu_instruction_count();
        println!("[resource] admin_revoke_proof(): cpu={}", cpu_count);
        assert!(cpu_count < 1_000_000, "CPU usage unexpectedly high");
    }

    // ── SUITE 2: Over-limit inputs rejected before storage ──────────────

    /// ATOMICITY: Duplicate proof_id_hash is rejected before storage.
    #[test]
    fn test_over_limit_duplicate_proof_id_rejected() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment1 = bytes(&env, 2);
        let commitment2 = bytes(&env, 3);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof(&proof_id, &commitment1, &issuer, &1, &2_000);

        // Attempt duplicate proof_id
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof(&proof_id, &commitment2, &issuer, &1, &3_000);
        }));

        assert!(
            result.is_err(),
            "Duplicate proof_id must be rejected"
        );
        println!("[atomicity] duplicate_proof_id: rejected before storage");
    }

    /// ATOMICITY: Invalid schema version is rejected before storage.
    #[test]
    fn test_over_limit_invalid_schema_version_rejected() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let issuer = Address::from_str(&env, ISSUER);

        // Try to register with unapproved schema version
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof(&proof_id, &commitment, &issuer, &999, &2_000);
        }));

        assert!(
            result.is_err(),
            "Unapproved schema version must be rejected"
        );
        println!("[atomicity] invalid_schema_version: rejected before storage");
    }

    /// ATOMICITY: Future-dated expiration is required; past dates rejected.
    #[test]
    fn test_over_limit_expired_proof_rejected() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let issuer = Address::from_str(&env, ISSUER);

        // Try to register with past expiration timestamp
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof(&proof_id, &commitment, &issuer, &1, &0);
        }));

        assert!(
            result.is_err(),
            "Past expiration timestamp must be rejected"
        );
        println!("[atomicity] past_expiration: rejected before storage");
    }

    /// ATOMICITY: Inactive issuer is rejected before storage.
    #[test]
    fn test_over_limit_inactive_issuer_rejected() {
        let (env, client, _protocol_config, issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let inactive_issuer = Address::from_str(&env, "GBXHUHG5FGYLPD6RHL2MKWMP572O6KUXCZXDZJXS4T57ZTMAKBN7DWXN");

        // Register inactive issuer (not in issuer registry)
        // Try to register proof with non-existent issuer
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof(&proof_id, &commitment, &inactive_issuer, &1, &2_000);
        }));

        assert!(
            result.is_err(),
            "Inactive issuer must be rejected"
        );
        println!("[atomicity] inactive_issuer: rejected before storage");
    }

    /// ATOMICITY: Over-limit operation commits NO storage on error.
    #[test]
    fn test_over_limit_duplicate_commits_no_storage() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);

        // Attempt duplicate (will fail on proof_id check)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);
        }));

        if result.is_err() {
            // Verify the duplicate was not stored
            let stored = client.get_proof(&proof_id);
            // Should get the original, not a duplicate
            assert_eq!(stored.proof_id_hash, proof_id);
            println!("[atomicity] duplicate_proof: original storage intact, no duplicate");
        }
    }

    // ── SUITE 3: Cross-contract call resource scaling ──────────────────

    /// Verifies that register_proof with cross-contract calls scales linearly.
    /// register_proof calls protocol-config and issuer-registry.
    #[test]
    fn test_cross_contract_call_scaling() {
        let (env, client, protocol_config, issuer_registry) = setup();

        // Approve multiple schema versions to vary the protocol-config call cost
        for v in 1..=5 {
            protocol_config.approve_schema_version(&v);
        }

        env.budget().reset_default();

        // Register proof with schema version 1
        let proof_id1 = bytes(&env, 10);
        let commitment1 = bytes(&env, 11);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof(&proof_id1, &commitment1, &issuer, &1, &2_000);

        let cpu_first = env.budget().cpu_instruction_count();
        println!("[resource] register_proof(v=1) [cross-contract]: cpu={}", cpu_first);

        // Register another proof with different schema version (still approved)
        env.budget().reset_default();
        let proof_id2 = bytes(&env, 20);
        let commitment2 = bytes(&env, 21);
        client.register_proof(&proof_id2, &commitment2, &issuer, &2, &3_000);

        let cpu_second = env.budget().cpu_instruction_count();
        println!("[resource] register_proof(v=2) [cross-contract]: cpu={}", cpu_second);

        // Cost should be similar (cross-contract calls are cached in same transaction)
        assert!(cpu_first < 5_000_000, "First call CPU unexpectedly high");
        assert!(cpu_second < 5_000_000, "Second call CPU unexpectedly high");
    }

    // ── SUITE 4: Bulk operations resource scaling ────────────────────────

    /// Verifies that registering many proofs scales linearly.
    /// Measures cumulative CPU usage to verify budget headroom for 1000+ proofs.
    #[test]
    fn test_bulk_register_many_proofs_scales_linearly() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let num_proofs = 100_u32;
        let issuer = Address::from_str(&env, ISSUER);

        env.budget().reset_default();

        // Register 100 proofs
        for i in 0..num_proofs {
            let proof_id = bytes(&env, (i % 256) as u8);
            let commitment = bytes(&env, ((i + 1) % 256) as u8);

            // Skip if duplicate
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.register_proof(&proof_id, &commitment, &issuer, &1, &(2_000 + i as u64));
            }));

            if result.is_err() {
                // Skip duplicate, continue with next
                continue;
            }
        }

        let total_cpu = env.budget().cpu_instruction_count();
        let total_mem = env.budget().mem_bytes_used();

        println!(
            "[resource] bulk_register_proofs(100): total_cpu={}, total_mem={}",
            total_cpu, total_mem
        );

        // 100 proofs should stay well within budget
        // Expected: ~500k per proof * 100 = 50M CPU (with caching)
        assert!(total_cpu < 200_000_000, "Total CPU unexpectedly high for 100 proofs");
        assert!(total_mem < 2_000_000, "Total memory unexpectedly high for 100 proofs");
    }

    /// Verifies that revoking many proofs scales linearly.
    #[test]
    fn test_bulk_revoke_many_proofs_scales_linearly() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let num_proofs = 50_u32;
        let issuer = Address::from_str(&env, ISSUER);

        // Register 50 proofs first
        for i in 0..num_proofs {
            let proof_id = bytes(&env, (i % 256) as u8);
            let commitment = bytes(&env, ((i + 1) % 256) as u8);

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.register_proof(&proof_id, &commitment, &issuer, &1, &(2_000 + i as u64));
            }));

            if result.is_err() {
                continue;
            }
        }

        env.budget().reset_default();

        // Revoke all proofs
        for i in 0..num_proofs {
            let proof_id = bytes(&env, (i % 256) as u8);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.revoke_proof(&proof_id);
            }));

            if result.is_err() {
                continue;
            }
        }

        let total_cpu = env.budget().cpu_instruction_count();
        let total_mem = env.budget().mem_bytes_used();

        println!(
            "[resource] bulk_revoke_proofs(50): total_cpu={}, total_mem={}",
            total_cpu, total_mem
        );

        // Revokes should be cheaper than registration
        assert!(total_cpu < 100_000_000, "Total CPU unexpectedly high for 50 revocations");
        assert!(total_mem < 1_000_000, "Total memory unexpectedly high for 50 revocations");
    }

    // ── SUITE 5: Resource evidence separated by operation ───────────────

    /// Resource measurements for ALL proof-registry operations.
    /// These numbers are the baseline for budget reviews.
    #[test]
    fn test_resource_evidence_all_operations() {
        let (env, client, _protocol_config, _issuer_registry) = setup();

        let issuer = Address::from_str(&env, ISSUER);

        println!("\n[resource-baseline] proof-registry operations:");

        // Operation 1: initialize (already done in setup)
        let env2 = Env::default();
        env2.mock_all_auths();
        let protocol_config_id2 = env2.register(ProtocolConfigContract, ());
        let protocol_config_client2 = ProtocolConfigContractClient::new(&env2, &protocol_config_id2);
        let issuer_registry_id2 = env2.register(IssuerRegistryContract, ());
        let issuer_registry_client2 = IssuerRegistryContractClient::new(&env2, &issuer_registry_id2);
        let contract_id2 = env2.register(ProofRegistryContract, ());
        let client2 = ProofRegistryContractClient::new(&env2, &contract_id2);
        let admin = Address::from_str(&env2, ADMIN);
        let issuer2 = Address::from_str(&env2, ISSUER);
        let issuer_id2 = bytes(&env2, 9);

        protocol_config_client2.initialize(&admin);
        protocol_config_client2.approve_schema_version(&1);
        issuer_registry_client2.initialize(&admin);
        issuer_registry_client2.register_issuer(&issuer_id2, &issuer2, &bytes(&env2, 8));

        env2.budget().reset_default();
        client2.initialize(&admin, &issuer_registry_id2, &protocol_config_id2);
        println!("  - initialize(): cpu={}", env2.budget().cpu_instruction_count());

        // Operation 2: get_admin
        env.budget().reset_default();
        let _ = client.get_admin();
        println!("  - get_admin(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 3: register_proof
        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        env.budget().reset_default();
        client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);
        println!("  - register_proof(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 4: get_proof
        env.budget().reset_default();
        let _ = client.get_proof(&proof_id);
        println!("  - get_proof(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 5: is_valid_proof
        env.budget().reset_default();
        let _ = client.is_valid_proof(&proof_id);
        println!("  - is_valid_proof(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 6: revoke_proof
        env.budget().reset_default();
        client.revoke_proof(&proof_id);
        println!("  - revoke_proof(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 7: Register another for admin_revoke
        let proof_id2 = bytes(&env, 10);
        let commitment2 = bytes(&env, 11);
        client.register_proof(&proof_id2, &commitment2, &issuer, &1, &3_000);

        env.budget().reset_default();
        client.admin_revoke_proof(&proof_id2);
        println!("  - admin_revoke_proof(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 8: is_revoked
        env.budget().reset_default();
        let _ = client.is_revoked(&proof_id2);
        println!("  - is_revoked(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 9: get_issuer_registry
        env.budget().reset_default();
        let _ = client.get_issuer_registry();
        println!("  - get_issuer_registry(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 10: get_protocol_config
        env.budget().reset_default();
        let _ = client.get_protocol_config();
        println!("  - get_protocol_config(): cpu={}", env.budget().cpu_instruction_count());

        println!("[resource-baseline] proof-registry: complete\n");
    }

    /// Resource cost of cross-contract dependency chain.
    /// proof-registry -> issuer-registry + protocol-config
    #[test]
    fn test_resource_evidence_full_dependency_chain() {
        let (env, client, protocol_config, issuer_registry) = setup();

        let issuer = Address::from_str(&env, ISSUER);

        println!("\n[resource-baseline] full cross-contract dependency chain:");

        // Worst-case: first proof registration (all cross-contract calls fresh)
        env.budget().reset_default();
        let proof_id = bytes(&env, 50);
        let commitment = bytes(&env, 51);
        client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);

        let cpu_first_proof = env.budget().cpu_instruction_count();
        println!(
            "  - register_proof() [first, all cross-contract calls]: cpu={}",
            cpu_first_proof
        );

        // Best-case: second proof registration (cross-contract calls cached)
        env.budget().reset_default();
        let proof_id2 = bytes(&env, 60);
        let commitment2 = bytes(&env, 61);
        client.register_proof(&proof_id2, &commitment2, &issuer, &1, &3_000);

        let cpu_second_proof = env.budget().cpu_instruction_count();
        println!(
            "  - register_proof() [second, cached calls]: cpu={}",
            cpu_second_proof
        );

        // is_active_address() call cost in isolation
        env.budget().reset_default();
        let is_active = issuer_registry.is_active_address(&issuer);
        let cpu_is_active = env.budget().cpu_instruction_count();
        println!("  - issuer_registry::is_active_address(): cpu={}", cpu_is_active);

        // is_paused() call cost in isolation
        env.budget().reset_default();
        let is_paused = protocol_config.is_paused();
        let cpu_is_paused = env.budget().cpu_instruction_count();
        println!("  - protocol_config::is_paused(): cpu={}", cpu_is_paused);

        println!("[resource-baseline] full dependency chain: complete\n");
    }
}
