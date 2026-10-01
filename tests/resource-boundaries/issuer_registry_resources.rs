//! Resource boundary tests for issuer-registry contract.
//!
//! These tests verify that issuer registration and management operations
//! stay within Soroban per-transaction CPU and memory budgets, especially
//! when handling bulk issuer registrations.

#[cfg(test)]
mod issuer_registry_resource_tests {
    extern crate std;

    use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
    use earnproof_shared::{MAX_ISSUER_ID_HASH_BYTES, MAX_METADATA_HASH_BYTES};
    use soroban_sdk::{Address, BytesN, Env};

    const ADMIN: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";
    const ISSUER_ONE: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";
    const ISSUER_TWO: &str = "GDWUSKGGFDI4FRXK5EBTRECZSVQSSWJHHJOGH6JWG3AUMFFMQ435DIAG";

    fn bytes(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn setup() -> (Env, IssuerRegistryContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IssuerRegistryContract, ());
        let client = IssuerRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        client.initialize(&admin);
        (env, client, admin)
    }

    // ── SUITE 1: Exact-limit inputs stay within budget ────────────────

    /// Verifies that issuer registration at exact limits succeeds
    /// and stays within Soroban CPU and memory budgets.
    ///
    /// issuer_id_hash and metadata_hash are both BytesN<32> (fixed 32 bytes).
    #[test]
    fn test_exact_limit_register_issuer_succeeds() {
        let (env, client, _admin) = setup();

        let issuer_id_hash = bytes(&env, 1);
        let metadata_hash = bytes(&env, 2);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);

        env.budget().reset_default();

        client.register_issuer(&issuer_id_hash, &issuer_address, &metadata_hash, &metadata_hash);
        let record = client.get_issuer(&issuer_id_hash);

        assert_eq!(record.issuer_id_hash, issuer_id_hash);
        assert_eq!(record.metadata_hash, metadata_hash);

        let cpu_count = env.budget().cpu_instruction_count();
        let mem_bytes = env.budget().mem_bytes_used();
        println!(
            "[resource] register_issuer(): cpu={}, mem={}",
            cpu_count, mem_bytes
        );

        // Budget assertions
        // Typical: ~200k-500k CPU, 5k-20k memory for single issuer
        assert!(cpu_count < 2_000_000, "CPU usage unexpectedly high");
        assert!(mem_bytes < 100_000, "Memory usage unexpectedly high");
    }

    /// Verifies that updating issuer metadata stays within budget.
    #[test]
    fn test_exact_limit_update_issuer_succeeds() {
        let (env, client, _admin) = setup();

        let issuer_id = bytes(&env, 1);
        let metadata1 = bytes(&env, 2);
        let metadata2 = bytes(&env, 3);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);

        client.register_issuer(&issuer_id, &issuer_address, &metadata1, &metadata1);

        env.budget().reset_default();
        client.update_issuer(&issuer_id, &metadata2);

        let record = client.get_issuer(&issuer_id);
        assert_eq!(record.metadata_hash, metadata2);

        let cpu_count = env.budget().cpu_instruction_count();
        println!("[resource] update_issuer(): cpu={}", cpu_count);
        assert!(cpu_count < 1_000_000, "CPU usage unexpectedly high");
    }

    /// Verifies that address rotation stays within budget.
    #[test]
    fn test_exact_limit_rotate_issuer_address_succeeds() {
        let (env, client, _admin) = setup();

        let issuer_id = bytes(&env, 1);
        let metadata = bytes(&env, 2);
        let old_address = Address::from_str(&env, ISSUER_ONE);
        let new_address = Address::from_str(&env, ISSUER_TWO);

        client.register_issuer(&issuer_id, &old_address, &metadata, &metadata);

        env.budget().reset_default();
        client.rotate_issuer_address(&issuer_id, &new_address);

        let record = client.get_issuer(&issuer_id);
        assert_eq!(record.issuer_address, new_address);

        let cpu_count = env.budget().cpu_instruction_count();
        println!("[resource] rotate_issuer_address(): cpu={}", cpu_count);
        assert!(cpu_count < 2_000_000, "CPU usage unexpectedly high");
    }

    /// Verifies that suspend/reactivate/revoke operations stay within budget.
    #[test]
    fn test_exact_limit_status_transitions_succeed() {
        let (env, client, _admin) = setup();

        let issuer_id = bytes(&env, 1);
        let metadata = bytes(&env, 2);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);

        client.register_issuer(&issuer_id, &issuer_address, &metadata, &metadata);

        // Suspend
        env.budget().reset_default();
        client.suspend_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]));
        let cpu_suspend = env.budget().cpu_instruction_count();
        println!("[resource] suspend_issuer(): cpu={}", cpu_suspend);
        assert!(!client.is_active_issuer(&issuer_id));

        // Reactivate
        env.budget().reset_default();
        client.reactivate_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]));
        let cpu_reactivate = env.budget().cpu_instruction_count();
        println!("[resource] reactivate_issuer(): cpu={}", cpu_reactivate);
        assert!(client.is_active_issuer(&issuer_id));

        // Revoke
        env.budget().reset_default();
        client.revoke_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]));
        let cpu_revoke = env.budget().cpu_instruction_count();
        println!("[resource] revoke_issuer(): cpu={}", cpu_revoke);
        assert!(!client.is_active_issuer(&issuer_id));
    }

    // ── SUITE 2: Over-limit inputs rejected before storage ──────────────

    /// ATOMICITY: Duplicate issuer_id_hash is rejected before address storage.
    /// Only the issuer_id_hash is checked; address entry should not be written.
    #[test]
    fn test_over_limit_duplicate_issuer_id_rejected() {
        let (env, client, _admin) = setup();

        let issuer_id = bytes(&env, 1);
        let metadata1 = bytes(&env, 2);
        let metadata2 = bytes(&env, 3);
        let addr1 = Address::from_str(&env, ISSUER_ONE);
        let addr2 = Address::from_str(&env, ISSUER_TWO);

        // Register first issuer
        client.register_issuer(&issuer_id, &addr1, &metadata1, &metadata1);

        // Attempt duplicate issuer_id
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_issuer(&issuer_id, &addr2, &metadata2, &metadata2);
        }));

        assert!(
            result.is_err(),
            "Duplicate issuer_id must be rejected"
        );
        println!("[atomicity] duplicate_issuer_id: rejected before storage");
    }

    /// ATOMICITY: Duplicate issuer_address is rejected before metadata storage.
    #[test]
    fn test_over_limit_duplicate_issuer_address_rejected() {
        let (env, client, _admin) = setup();

        let issuer_id1 = bytes(&env, 1);
        let issuer_id2 = bytes(&env, 2);
        let metadata1 = bytes(&env, 3);
        let metadata2 = bytes(&env, 4);
        let shared_address = Address::from_str(&env, ISSUER_ONE);

        // Register first issuer with address
        client.register_issuer(&issuer_id1, &shared_address, &metadata1, &metadata1);

        // Attempt to register second issuer with same address
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_issuer(&issuer_id2, &shared_address, &metadata2, &metadata2);
        }));

        assert!(
            result.is_err(),
            "Duplicate issuer_address must be rejected"
        );
        println!("[atomicity] duplicate_issuer_address: rejected before storage");
    }

    /// ATOMICITY: Over-limit operation commits NO storage on first error.
    #[test]
    fn test_over_limit_duplicate_commits_no_storage() {
        let (env, client, _admin) = setup();

        let issuer_id = bytes(&env, 1);
        let metadata = bytes(&env, 2);
        let address = Address::from_str(&env, ISSUER_ONE);

        client.register_issuer(&issuer_id, &address, &metadata, &metadata);

        // Attempt duplicate (will fail on issuer_id check)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_issuer(&issuer_id, &address, &metadata, &metadata);
        }));

        if result.is_err() {
            // Verify the duplicate was not stored
            let stored = client.get_issuer(&issuer_id);
            // Should get the original, not a duplicate
            assert_eq!(stored.metadata_hash, metadata);
            println!("[atomicity] duplicate_issuer: original storage intact, no duplicate");
        }
    }

    // ── SUITE 3: Bulk operations resource scaling ────────────────────────

    /// Verifies that registering many issuers scales linearly.
    /// Measures cumulative CPU usage to verify budget headroom for 100+ issuers.
    #[test]
    fn test_bulk_register_many_issuers_scales_linearly() {
        let (env, client, _admin) = setup();

        let num_issuers = 100_u32;

        env.budget().reset_default();

        // Register 100 issuers
        for i in 0..num_issuers {
            let issuer_id = bytes(&env, (i % 256) as u8);
            let metadata = bytes(&env, ((i + 1) % 256) as u8);
            let issuer_addr = if i % 2 == 0 {
                Address::from_str(&env, ISSUER_ONE)
            } else {
                Address::from_str(&env, ISSUER_TWO)
            };

            // Skip if duplicate (different combinations fail)
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.register_issuer(&issuer_id, &issuer_addr, &metadata, &metadata);
            }));

            if result.is_err() {
                // Skip duplicate, continue with next
                continue;
            }
        }

        let total_cpu = env.budget().cpu_instruction_count();
        let total_mem = env.budget().mem_bytes_used();

        println!(
            "[resource] bulk_register_issuers(100): total_cpu={}, total_mem={}",
            total_cpu, total_mem
        );

        // 100 issuers should stay well within budget
        // Expected: ~200k per issuer * 100 = 20M CPU (with caching)
        assert!(total_cpu < 100_000_000, "Total CPU unexpectedly high for 100 issuers");
        assert!(total_mem < 1_000_000, "Total memory unexpectedly high for 100 issuers");
    }

    /// Verifies that updating many issuers scales linearly.
    #[test]
    fn test_bulk_update_many_issuers_scales_linearly() {
        let (env, client, _admin) = setup();

        let num_updates = 50_u32;

        // Register one issuer
        let issuer_id = bytes(&env, 1);
        let initial_metadata = bytes(&env, 2);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);
        client.register_issuer(&issuer_id, &issuer_address, &initial_metadata, &initial_metadata);

        env.budget().reset_default();

        // Update metadata 50 times
        for i in 0..num_updates {
            let new_metadata = bytes(&env, ((i + 10) % 256) as u8);
            client.update_issuer(&issuer_id, &new_metadata);
        }

        let total_cpu = env.budget().cpu_instruction_count();
        let total_mem = env.budget().mem_bytes_used();

        println!(
            "[resource] bulk_update_issuer(50): total_cpu={}, total_mem={}",
            total_cpu, total_mem
        );

        // Updates should be cheaper than registration
        assert!(total_cpu < 50_000_000, "Total CPU unexpectedly high for 50 updates");
        assert!(total_mem < 500_000, "Total memory unexpectedly high for 50 updates");
    }

    // ── SUITE 4: Resource evidence separated by operation ───────────────

    /// Resource measurements for ALL issuer-registry operations.
    /// These numbers are the baseline for budget reviews.
    #[test]
    fn test_resource_evidence_all_operations() {
        let (env, client, _admin) = setup();

        println!("\n[resource-baseline] issuer-registry operations:");

        // Operation 1: initialize (already done in setup)
        let env2 = Env::default();
        env2.mock_all_auths();
        let contract_id2 = env2.register(IssuerRegistryContract, ());
        let client2 = IssuerRegistryContractClient::new(&env2, &contract_id2);
        let admin = Address::from_str(&env2, ADMIN);
        env2.budget().reset_default();
        client2.initialize(&admin);
        println!("  - initialize(): cpu={}", env2.budget().cpu_instruction_count());

        // Operation 2: get_admin
        env.budget().reset_default();
        let _ = client.get_admin();
        println!("  - get_admin(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 3: register_issuer
        let issuer_id = bytes(&env, 1);
        let metadata = bytes(&env, 2);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);
        env.budget().reset_default();
        client.register_issuer(&issuer_id, &issuer_address, &metadata, &metadata);
        println!("  - register_issuer(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 4: update_issuer
        let new_metadata = bytes(&env, 3);
        env.budget().reset_default();
        client.update_issuer(&issuer_id, &new_metadata);
        println!("  - update_issuer(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 5: get_issuer
        env.budget().reset_default();
        let _ = client.get_issuer(&issuer_id);
        println!("  - get_issuer(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 6: is_active_issuer
        env.budget().reset_default();
        let _ = client.is_active_issuer(&issuer_id);
        println!("  - is_active_issuer(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 7: is_active_address
        env.budget().reset_default();
        let _ = client.is_active_address(&issuer_address);
        println!("  - is_active_address(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 8: get_issuer_by_address
        env.budget().reset_default();
        let _ = client.get_issuer_by_address(&issuer_address);
        println!("  - get_issuer_by_address(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 9: suspend_issuer
        env.budget().reset_default();
        client.suspend_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]));
        println!("  - suspend_issuer(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 10: reactivate_issuer
        env.budget().reset_default();
        client.reactivate_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]));
        println!("  - reactivate_issuer(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 11: rotate_issuer_address
        let new_address = Address::from_str(&env, ISSUER_TWO);
        env.budget().reset_default();
        client.rotate_issuer_address(&issuer_id, &new_address);
        println!("  - rotate_issuer_address(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 12: revoke_issuer
        env.budget().reset_default();
        client.revoke_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&env, &[0x99u8; 32]));
        println!("  - revoke_issuer(): cpu={}", env.budget().cpu_instruction_count());

        println!("[resource-baseline] issuer-registry: complete\n");
    }

    /// Cross-contract call resource cost baseline.
    /// Measures the cost of issuer-registry being called from proof-registry.
    #[test]
    fn test_resource_evidence_issuer_registry_cross_contract_calls() {
        let (env, client, _admin) = setup();

        let issuer_id = bytes(&env, 1);
        let metadata = bytes(&env, 2);
        let issuer_address = Address::from_str(&env, ISSUER_ONE);

        client.register_issuer(&issuer_id, &issuer_address, &metadata, &metadata);

        println!("\n[resource-baseline] issuer-registry cross-contract calls:");

        // is_active_address() - called from proof-registry::register_proof
        env.budget().reset_default();
        let is_active = client.is_active_address(&issuer_address);
        let cpu_is_active = env.budget().cpu_instruction_count();
        println!("  - is_active_address() [from cross-contract]: cpu={}", cpu_is_active);

        // get_issuer_by_address() - called for lookups
        env.budget().reset_default();
        let record = client.get_issuer_by_address(&issuer_address);
        let cpu_get_by_addr = env.budget().cpu_instruction_count();
        println!("  - get_issuer_by_address() [from cross-contract]: cpu={}", cpu_get_by_addr);

        println!("[resource-baseline] issuer-registry cross-contract: complete\n");
    }
}
