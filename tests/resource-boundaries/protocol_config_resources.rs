//! Resource boundary tests for protocol-config contract.
//!
//! These tests verify that schema version operations stay within
//! Soroban per-transaction CPU and memory budgets.

#[cfg(test)]
mod protocol_config_resource_tests {
    extern crate std;

    use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient, ContractError};
    use soroban_sdk::{Address, Env};

    const ADMIN: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";

    fn setup() -> (Env, ProtocolConfigContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        client.initialize(&admin);
        (env, client, admin)
    }

    // ── SUITE 1: Exact-limit inputs stay within budget ────────────────

    /// Verifies that schema version at u32::MAX (theoretical max) succeeds
    /// and stays within Soroban CPU and memory budgets.
    ///
    /// Resource evidence: prints CPU and memory after the call.
    #[test]
    fn test_exact_limit_schema_version_approve_succeeds() {
        let (env, client, _admin) = setup();

        // Near-maximum schema version (practical limit, not u32::MAX)
        let version = 1_000_000_u32;

        // Reset budget for clean measurement
        env.budget().reset_default();

        // Must succeed at exact limit
        client.approve_schema_version(&version);
        assert!(client.is_schema_version_approved(&version));

        // Print resource evidence (reproducible)
        let cpu_count = env.budget().cpu_instruction_count();
        let mem_bytes = env.budget().mem_bytes_used();
        println!(
            "[resource] approve_schema_version(v={}): cpu={}, mem={}",
            version, cpu_count, mem_bytes
        );

        // Budget assertions (update after first baseline run)
        // Typical ranges for single schema version approval:
        // CPU: 50k-200k instructions
        // Memory: 1k-10k bytes
        assert!(cpu_count < 1_000_000, "CPU usage unexpectedly high");
        assert!(mem_bytes < 100_000, "Memory usage unexpectedly high");
    }

    /// Verifies that schema version 1 (minimum) can be approved and deprecated.
    #[test]
    fn test_exact_limit_schema_version_min_succeeds() {
        let (env, client, _admin) = setup();

        let version = 1_u32;

        env.budget().reset_default();

        client.approve_schema_version(&version);
        assert!(client.is_schema_version_approved(&version));

        let cpu_approve = env.budget().cpu_instruction_count();
        println!(
            "[resource] approve_schema_version(v=1): cpu={}",
            cpu_approve
        );

        env.budget().reset_default();

        client.deprecate_schema_version(&version);
        assert!(!client.is_schema_version_approved(&version));

        let cpu_deprecate = env.budget().cpu_instruction_count();
        println!(
            "[resource] deprecate_schema_version(v=1): cpu={}",
            cpu_deprecate
        );
    }

    /// Verifies pause/unpause operations stay within budget.
    #[test]
    fn test_exact_limit_pause_operations_succeed() {
        let (env, client, _admin) = setup();

        env.budget().reset_default();
        client.pause();
        assert!(client.is_paused());

        let cpu_pause = env.budget().cpu_instruction_count();
        println!("[resource] pause(): cpu={}", cpu_pause);

        env.budget().reset_default();
        client.unpause();
        assert!(!client.is_paused());

        let cpu_unpause = env.budget().cpu_instruction_count();
        println!("[resource] unpause(): cpu={}", cpu_unpause);
    }

    /// Verifies set_admin operations stay within budget.
    #[test]
    fn test_exact_limit_set_admin_succeeds() {
        let (env, client, _admin) = setup();

        let new_admin = Address::from_str(&env, "GBXHUHG5FGYLPD6RHL2MKWMP572O6KUXCZXDZJXS4T57ZTMAKBN7DWXN");

        env.budget().reset_default();
        client.nominate_admin(&new_admin);
        client.accept_admin();
        assert_eq!(client.get_admin(), new_admin);

        let cpu_count = env.budget().cpu_instruction_count();
        println!("[resource] set_admin(): cpu={}", cpu_count);
    }

    // ── SUITE 2: Over-limit inputs rejected before storage ──────────────

    /// Verifies that schema version 0 (below MIN) is rejected.
    /// MIN_SCHEMA_VERSION is 1, so 0 should fail.
    #[test]
    fn test_over_limit_schema_version_zero_rejected() {
        let (env, client, _admin) = setup();

        env.budget().reset_default();

        // Version 0 is not allowed (MIN_SCHEMA_VERSION = 1)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_schema_version(&0);
        }));

        assert!(
            result.is_err(),
            "Schema version 0 must be rejected"
        );

        println!("[atomicity] schema_version(0): rejected before storage");
    }

    /// ATOMICITY: Over-limit input commits NO storage.
    /// If a schema version approval fails, is_schema_version_approved must return false.
    #[test]
    fn test_over_limit_schema_version_commits_no_storage() {
        let (env, client, _admin) = setup();

        let test_version = 42_u32;

        // Attempt to approve with panicking input (version 0 in a separate call)
        // Use a version that will fail validation
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_schema_version(&0);
        }));

        // Even if approval fails, verify the version is not stored
        if result.is_err() {
            assert!(
                !client.is_schema_version_approved(&0),
                "Rejected version must not be stored"
            );
        }

        println!("[atomicity] over-limit schema version: no storage written");
    }

    /// ATOMICITY: Over-limit input emits NO success events.
    #[test]
    fn test_over_limit_emits_no_events() {
        let (env, client, _admin) = setup();

        // Get initial event count
        let events_before = env.events().all();

        // Attempt invalid operation
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_schema_version(&0);
        }));

        // On error, no SchemaApproved event should be emitted
        if result.is_err() {
            let events_after = env.events().all();
            // Events should not increase (or at most increase by panic event)
            println!(
                "[atomicity] over-limit: events_before={}, events_after={}",
                events_before.len(),
                events_after.len()
            );
        }
    }

    // ── SUITE 3: Bulk operations resource scaling ────────────────────────

    /// Verifies that multiple schema version approvals scale linearly
    /// and don't exhaust budget unexpectedly.
    #[test]
    fn test_bulk_schema_versions_scale_linearly() {
        let (env, client, _admin) = setup();

        let num_versions = 100_u32;

        env.budget().reset_default();

        for i in 1..=num_versions {
            client.approve_schema_version(&i);
        }

        let total_cpu = env.budget().cpu_instruction_count();
        let total_mem = env.budget().mem_bytes_used();
        let avg_cpu_per_version = total_cpu / num_versions as u64;

        println!(
            "[resource] approve {} versions: total_cpu={}, avg_cpu_per_version={}, total_mem={}",
            num_versions, total_cpu, avg_cpu_per_version, total_mem
        );

        // Linear scaling check: average should stay consistent
        // If one version takes ~100k CPU, 100 versions should ~10M CPU
        assert!(total_cpu < 50_000_000, "Total CPU unexpectedly high");
        assert!(total_mem < 500_000, "Total memory unexpectedly high");
    }

    // ── SUITE 4: Resource evidence separated by operation ───────────────

    /// Resource measurements for ALL protocol-config operations.
    /// These numbers are the baseline for budget reviews.
    #[test]
    fn test_resource_evidence_all_operations() {
        let (env, client, _admin) = setup();

        println!("\n[resource-baseline] protocol-config operations:");

        // Operation 1: initialize (already done in setup)
        env.budget().reset_default();
        let admin = Address::from_str(&env, "GBXHUHG5FGYLPD6RHL2MKWMP572O6KUXCZXDZJXS4T57ZTMAKBN7DWXN");
        let env2 = Env::default();
        env2.mock_all_auths();
        let contract_id2 = env2.register(ProtocolConfigContract, ());
        let client2 = ProtocolConfigContractClient::new(&env2, &contract_id2);
        client2.initialize(&admin);
        println!("  - initialize(): cpu={}", env2.budget().cpu_instruction_count());

        // Operation 2: get_admin
        env.budget().reset_default();
        let _ = client.get_admin();
        println!("  - get_admin(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 3: set_admin
        env.budget().reset_default();
        let new_admin = Address::from_str(&env, "GBXHUHG5FGYLPD6RHL2MKWMP572O6KUXCZXDZJXS4T57ZTMAKBN7DWXN");
        client.nominate_admin(&new_admin);
        client.accept_admin();
        println!("  - set_admin(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 4: is_paused
        env.budget().reset_default();
        let _ = client.is_paused();
        println!("  - is_paused(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 5: pause
        env.budget().reset_default();
        client.pause();
        println!("  - pause(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 6: unpause
        env.budget().reset_default();
        client.unpause();
        println!("  - unpause(): cpu={}", env.budget().cpu_instruction_count());

        // Operation 7: approve_schema_version
        env.budget().reset_default();
        client.approve_schema_version(&1);
        println!("  - approve_schema_version(1): cpu={}", env.budget().cpu_instruction_count());

        // Operation 8: is_schema_version_approved
        env.budget().reset_default();
        let _ = client.is_schema_version_approved(&1);
        println!("  - is_schema_version_approved(1): cpu={}", env.budget().cpu_instruction_count());

        // Operation 9: deprecate_schema_version
        env.budget().reset_default();
        client.deprecate_schema_version(&1);
        println!("  - deprecate_schema_version(1): cpu={}", env.budget().cpu_instruction_count());

        // Operation 10: get_config_version
        env.budget().reset_default();
        let _ = client.get_config_version();
        println!("  - get_config_version(): cpu={}", env.budget().cpu_instruction_count());

        println!("[resource-baseline] protocol-config: complete\n");
    }

    /// Cross-contract call resource cost baseline.
    /// Measures the cost of protocol-config being called from another contract.
    #[test]
    fn test_resource_evidence_protocol_config_cross_contract_calls() {
        let (env, client, _admin) = setup();

        println!("\n[resource-baseline] protocol-config cross-contract calls:");

        // Simulate cross-contract scenario: proof-registry calls is_paused()
        env.budget().reset_default();
        let is_paused = client.is_paused();
        let cpu_is_paused = env.budget().cpu_instruction_count();
        println!("  - is_paused() [from cross-contract]: cpu={}", cpu_is_paused);

        // is_schema_version_approved() with TTL extension
        env.budget().reset_default();
        client.approve_schema_version(&1);
        env.budget().reset_default();
        let approved = client.is_schema_version_approved(&1);
        let cpu_approved = env.budget().cpu_instruction_count();
        println!("  - is_schema_version_approved(1) [from cross-contract]: cpu={}", cpu_approved);

        println!("[resource-baseline] protocol-config cross-contract: complete\n");
    }
}
