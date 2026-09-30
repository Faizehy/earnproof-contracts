use earnproof_shared::{IssuerStatus, ProofStatus};
use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use proptest::prelude::*;
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
use soroban_sdk::{testutils::Ledger as _, Address, BytesN, Env};

const ADMIN: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";
const ISSUER: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";
const ISSUER_TWO: &str = "GDWUSKGGFDI4FRXK5EBTRECZSVQSSWJHHJOGH6JWG3AUMFFMQ435DIAG";

fn bytes(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}

fn try_op(_env: &Env, op: impl FnOnce()) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(op)).is_ok()
}

fn setup_issuer() -> (
    Env,
    IssuerRegistryContractClient<'static>,
    Address,
    BytesN<32>,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(IssuerRegistryContract, ());
    let client = IssuerRegistryContractClient::new(&env, &contract_id);
    let admin = Address::from_str(&env, ADMIN);
    client.initialize(&admin);
    let issuer_id = bytes(&env, 1);
    let issuer_address = Address::from_str(&env, ISSUER);
    let metadata = bytes(&env, 2);
    client.register_issuer(&issuer_id, &issuer_address, &metadata, &metadata);
    (env, client, admin, issuer_id, issuer_address)
}

fn setup_proof() -> (
    Env,
    ProofRegistryContractClient<'static>,
    ProtocolConfigContractClient<'static>,
    IssuerRegistryContractClient<'static>,
    Address,
    Address,
    u64,
) {
    let env = Env::default();
    env.mock_all_auths();
    let base_time = 1_000_u64;
    env.ledger().set_timestamp(base_time);
    let protocol_id = env.register(ProtocolConfigContract, ());
    let protocol = ProtocolConfigContractClient::new(&env, &protocol_id);
    let issuer_registry_id = env.register(IssuerRegistryContract, ());
    let issuer_registry = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
    let proof_registry_id = env.register(ProofRegistryContract, ());
    let proof = ProofRegistryContractClient::new(&env, &proof_registry_id);
    let admin = Address::from_str(&env, ADMIN);
    let issuer_address = Address::from_str(&env, ISSUER);
    protocol.initialize(&admin);
    protocol.approve_schema_version(&1);
    issuer_registry.initialize(&admin);
    issuer_registry.register_issuer(
        &bytes(&env, 9),
        &issuer_address,
        &bytes(&env, 8),
        &bytes(&env, 99),
    );
    proof.initialize(&admin, &issuer_registry_id, &protocol_id);
    (
        env,
        proof,
        protocol,
        issuer_registry,
        admin,
        issuer_address,
        base_time,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn issuer_status_never_reactivates_after_revoke(
        ops in prop::collection::vec(any::<u8>(), 1..20),
    ) {
        let (env, client, _admin, issuer_id, _issuer_address) = setup_issuer();
        let mut status = IssuerStatus::Active;

        for (i, op) in ops.into_iter().enumerate() {
            let op_kind = op % 3;
            let expected_success = match (op_kind, &status) {
                (0, IssuerStatus::Revoked) => false, // suspend
                (1, IssuerStatus::Revoked) => false, // reactivate
                _ => true,
            };

            let before = client.get_issuer(&issuer_id);
            let success = try_op(&env, || match op_kind {
                0 => { client.suspend_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&client.env, &[1u8; 32])); }
                1 => { client.reactivate_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&client.env, &[1u8; 32])); }
                _ => { client.revoke_issuer(&issuer_id, &soroban_sdk::BytesN::from_array(&client.env, &[1u8; 32])); }
            });
            prop_assert_eq!(success, expected_success, "iteration {}", i);

            if success {
                match op_kind {
                    0 => status = IssuerStatus::Suspended,
                    1 => status = IssuerStatus::Active,
                    _ => status = IssuerStatus::Revoked,
                }
            } else {
                let after = client.get_issuer(&issuer_id);
                prop_assert_eq!(after.status, before.status, "status changed on failed op");
                prop_assert_eq!(after.updated_at, before.updated_at, "updated_at changed on failed op");
            }

            let record = client.get_issuer(&issuer_id);
            prop_assert_eq!(record.status, status.clone());
        }
    }

    #[test]
    fn duplicate_registration_preserves_original(duplicate_id in any::<bool>()) {
        let (env, client, _admin, issuer_id, issuer_address) = setup_issuer();
        let other_id = bytes(&env, 3);
        let other_address = Address::from_str(&env, ISSUER_TWO);
        let duplicate_metadata = bytes(&env, 4);
        let original_metadata = client.get_issuer(&issuer_id).metadata_hash;

        let result = if duplicate_id {
            try_op(&env, || {
                client.register_issuer(&issuer_id, &other_address, &duplicate_metadata, &duplicate_metadata);
            })
        } else {
            try_op(&env, || {
                client.register_issuer(&other_id, &issuer_address, &duplicate_metadata, &duplicate_metadata);
            })
        };
        prop_assert!(!result);

        let record = client.get_issuer(&issuer_id);
        prop_assert_eq!(record.metadata_hash, original_metadata);
        prop_assert_eq!(record.issuer_address, issuer_address);
    }

    #[test]
    fn proof_validity_false_after_revocation_or_expiration(
        expires_delta in 1_u64..10_000,
        action in 0_u8..3,
    ) {
        let (env, proof, _protocol, _issuer_registry, _admin, issuer_address, base_time) = setup_proof();
        let proof_id = bytes(&env, 1);
        let expires_at = base_time + expires_delta;
        let success = try_op(&env, || {
            proof.register_proof(&proof_id, &bytes(&env, 2), &issuer_address, &1, &expires_at);
        });
        prop_assert!(success);

        match action {
            0 => {
                try_op(&env, || { proof.revoke_proof(&proof_id); });
            }
            1 => {
                try_op(&env, || { proof.admin_revoke_proof(&proof_id); });
            }
            _ => {
                env.ledger().set_timestamp(expires_at + 1);
            }
        }

        let record = proof.get_proof(&proof_id);
        let now = env.ledger().timestamp();
        if record.status == ProofStatus::Revoked || now > record.expires_at {
            prop_assert!(!proof.is_valid_proof(&proof_id));
        }
    }

    #[test]
    fn paused_protocol_blocks_new_registration(
        pauses in prop::collection::vec(any::<bool>(), 0..10),
    ) {
        let (env, proof, protocol, _issuer_registry, _admin, issuer_address, _base_time) = setup_proof();
        for pause in pauses {
            if pause {
                try_op(&env, || { protocol.pause(); });
            } else {
                try_op(&env, || { protocol.unpause(); });
            }
        }

        let proof_id = bytes(&env, 1);
        let success = try_op(&env, || {
            proof.register_proof(&proof_id, &bytes(&env, 2), &issuer_address, &1, &2_000);
        });
        let exists = try_op(&env, || { proof.get_proof(&proof_id); });

        if protocol.is_paused() {
            prop_assert!(!success);
            prop_assert!(!exists);
        } else {
            prop_assert!(success);
            prop_assert!(exists);
        }
    }

    #[test]
    fn schema_and_admin_rotation_invariants(
        ops in prop::collection::vec(any::<u8>(), 1..20),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let other_admin = Address::from_str(&env, ISSUER_TWO);
        client.initialize(&admin);

        let mut current_admin = admin.clone();
        let mut approved = false;

        for op in ops {
            match op % 2 {
                0 => {
                    let new_admin = if current_admin == admin {
                        other_admin.clone()
                    } else {
                        admin.clone()
                    };
                    let result = try_op(&env, || { { client.nominate_admin(&new_admin); client.accept_admin(); } });
                    prop_assert!(result);
                    current_admin = new_admin;
                }
                _ => {
                    if approved {
                        let result = try_op(&env, || { client.deprecate_schema_version(&1); });
                        prop_assert!(result);
                        approved = false;
                    } else {
                        let result = try_op(&env, || { client.approve_schema_version(&1); });
                        prop_assert!(result);
                        approved = true;
                    }
                }
            }
            prop_assert_eq!(client.get_admin(), current_admin.clone());
            prop_assert_eq!(client.is_schema_version_approved(&1), approved);
        }
    }
}
