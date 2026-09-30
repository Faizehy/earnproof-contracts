//! Proof Registry TTL Boundary Tests

#[cfg(test)]
mod tests {
    extern crate std;

    use crate::harness::TtlTestHarness;
    use earnproof_shared::{ProofStatus, TTL_THRESHOLD_LEDGERS};
    use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
    use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
    use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
    use soroban_sdk::{Address, BytesN, Env};

    const ADMIN: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";
    const ISSUER: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";

    fn bytes(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn admin_addr(env: &Env) -> Address {
        Address::from_str(env, ADMIN)
    }

    fn issuer_addr(env: &Env) -> Address {
        Address::from_str(env, ISSUER)
    }

    fn setup(
        env: &Env,
    ) -> (
        ProofRegistryContractClient<'static>,
        IssuerRegistryContractClient<'static>,
        ProtocolConfigContractClient<'static>,
        Address,
    ) {
        env.mock_all_auths();

        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let protocol_config_client = ProtocolConfigContractClient::new(env, &protocol_config_id);
        let admin = admin_addr(env);
        protocol_config_client.initialize(&admin);
        protocol_config_client.approve_schema_version(&1);

        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let issuer_registry_client = IssuerRegistryContractClient::new(env, &issuer_registry_id);
        issuer_registry_client.initialize(&admin);
        let issuer = issuer_addr(env);
        let issuer_id = bytes(env, 9);
        issuer_registry_client.register_issuer(
            &issuer_id,
            &issuer,
            &bytes(env, 8),
            &bytes(env, 99),
        );

        let proof_registry_id = env.register(ProofRegistryContract, ());
        let proof_registry_client = ProofRegistryContractClient::new(env, &proof_registry_id);
        proof_registry_client.initialize(&admin, &issuer_registry_id, &protocol_config_id);

        (
            proof_registry_client,
            issuer_registry_client,
            protocol_config_client,
            issuer,
        )
    }

    // ── Instance Storage (Admin) ────

    #[test]
    fn instance_admin_pre_expiry_readable() {
        let env = Env::default();
        let (client, _issuer_reg, _protocol_config, _issuer) = setup(&env);

        let current_ledger = TtlTestHarness::current_ledger(&env);
        let expiry =
            TtlTestHarness::calculate_expiry(current_ledger, TTL_THRESHOLD_LEDGERS, 500_000);
        let pre_expiry = TtlTestHarness::pre_expiry_ledger(expiry);
        TtlTestHarness::advance_to_ledger(&env, pre_expiry);

        let admin = admin_addr(&env);
        let retrieved = client.get_admin();
        assert_eq!(retrieved, admin);
    }

    /// Post-expiry: the SDK 27 test host auto-restores expired persistent
    /// entries on access, so the admin read still succeeds.
    #[test]
    fn instance_admin_post_expiry_auto_restored() {
        let env = Env::default();
        let (client, _issuer_reg, _protocol_config, _issuer) = setup(&env);

        let current_ledger = TtlTestHarness::current_ledger(&env);
        let expiry =
            TtlTestHarness::calculate_expiry(current_ledger, TTL_THRESHOLD_LEDGERS, 500_000);
        let post_expiry = TtlTestHarness::post_expiry_ledger(expiry);
        TtlTestHarness::advance_to_ledger(&env, post_expiry);

        let result = client.try_get_admin();
        assert_eq!(
            result,
            Ok(Ok(admin_addr(&env))),
            "test host auto-restores expired persistent entries"
        );
    }

    // ── Persistent Storage: Proof(hash) ────

    #[test]
    fn persistent_proof_record_pre_expiry_readable() {
        let env = Env::default();
        let (client, _issuer_reg, _protocol_config, issuer) = setup(&env);

        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let expires_at = 2_000;
        client.register_proof(&proof_id, &commitment, &issuer, &1, &expires_at);

        let current_ledger = TtlTestHarness::current_ledger(&env);
        let expiry =
            TtlTestHarness::calculate_expiry(current_ledger, TTL_THRESHOLD_LEDGERS, 500_000);
        let pre_expiry = TtlTestHarness::pre_expiry_ledger(expiry);
        TtlTestHarness::advance_to_ledger(&env, pre_expiry);

        let record = client.get_proof(&proof_id);
        assert_eq!(record.proof_id_hash, proof_id);
        assert_eq!(record.status, ProofStatus::Active);
    }

    /// Post-expiry: the SDK 27 test host auto-restores expired persistent
    /// entries on access, so the proof record read still succeeds.
    #[test]
    fn persistent_proof_record_post_expiry_auto_restored() {
        let env = Env::default();
        let (client, _issuer_reg, _protocol_config, issuer) = setup(&env);

        let proof_id = bytes(&env, 5);
        let commitment = bytes(&env, 6);
        let expires_at = 2_000;
        client.register_proof(&proof_id, &commitment, &issuer, &1, &expires_at);

        let current_ledger = TtlTestHarness::current_ledger(&env);
        let expiry =
            TtlTestHarness::calculate_expiry(current_ledger, TTL_THRESHOLD_LEDGERS, 500_000);
        let post_expiry = TtlTestHarness::post_expiry_ledger(expiry);
        TtlTestHarness::advance_to_ledger(&env, post_expiry);

        let result = client.try_get_proof(&proof_id);
        let record = result
            .expect("read succeeds after auto-restore")
            .expect("proof found");
        assert_eq!(
            record.proof_id_hash, proof_id,
            "test host auto-restores expired persistent entries"
        );
    }

    // ── is_valid_proof: Cross-Contract Dependency ────

    #[test]
    fn is_valid_proof_false_when_storage_expired() {
        let env = Env::default();
        let (client, _issuer_reg, _protocol_config, issuer) = setup(&env);

        let proof_id = bytes(&env, 7);
        let commitment = bytes(&env, 8);
        let expires_at = 5_000;
        client.register_proof(&proof_id, &commitment, &issuer, &1, &expires_at);

        assert!(client.is_valid_proof(&proof_id));

        let current_ledger = TtlTestHarness::current_ledger(&env);
        let expiry =
            TtlTestHarness::calculate_expiry(current_ledger, TTL_THRESHOLD_LEDGERS, 500_000);
        let post_expiry = TtlTestHarness::post_expiry_ledger(expiry);
        TtlTestHarness::advance_to_ledger(&env, post_expiry);

        assert!(!client.is_valid_proof(&proof_id));
    }

    #[test]
    fn proof_verification_fails_when_issuer_expired() {
        let env = Env::default();
        let (client, _issuer_reg, _protocol_config, issuer) = setup(&env);

        let proof_id = bytes(&env, 15);
        let commitment = bytes(&env, 16);
        let expires_at = 5_000;
        client.register_proof(&proof_id, &commitment, &issuer, &1, &expires_at);

        assert!(client.is_valid_proof(&proof_id));

        let current_ledger = TtlTestHarness::current_ledger(&env);
        let issuer_expiry =
            TtlTestHarness::calculate_expiry(current_ledger, TTL_THRESHOLD_LEDGERS, 500_000);
        let issuer_post_expiry = TtlTestHarness::post_expiry_ledger(issuer_expiry);
        TtlTestHarness::advance_to_ledger(&env, issuer_post_expiry);

        assert!(!client.is_valid_proof(&proof_id));
    }
}
