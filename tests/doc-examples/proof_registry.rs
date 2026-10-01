//! Proof Registry Contract Invocation Examples
//!
//! This module contains executable documentation examples demonstrating
//! how to invoke the ProofRegistry contract methods. All examples use
//! synthetic identifiers and run in a local sandbox environment.
//!
//! # Examples
//!
//! ## Initialization
//!
//! Initialize the proof registry contract with admin and cross-contract references:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, Env};
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! fn example_initialize_proof_registry() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProofRegistryContract, ());
//!     let client = ProofRegistryContractClient::new(&env, &contract_id);
//!
//!     // Use synthetic addresses for "sandbox" network
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer_registry = Address::from_str(&env, "CB73TVWVJIIVNTKLWSHZB5NL2UIF3B3EUL4YH4MUD6EYX6SFIHE77D2F");
//!     let protocol_config = Address::from_str(&env, "CC3OREX5QBIKJ5JOW36JFJJW7TLAKJOVT5WJXEITGALO7MU32KHICS2A");
//!
//!     // Initialize with cross-contract references
//!     client.initialize(&admin, &issuer_registry, &protocol_config);
//!
//!     // Verify initialization
//!     assert_eq!(client.get_admin(), admin);
//!     assert_eq!(client.get_issuer_registry(), issuer_registry);
//!     assert_eq!(client.get_protocol_config(), protocol_config);
//! }
//! ```
//!
//! ## Register Proof
//!
//! Register a proof commitment with schema validation and expiration:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, BytesN, Env};
//! use earnproof_shared::ProofStatus;
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! fn example_register_proof() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProofRegistryContract, ());
//!     let client = ProofRegistryContractClient::new(&env, &contract_id);
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer_registry = Address::from_str(&env, "CB73TVWVJIIVNTKLWSHZB5NL2UIF3B3EUL4YH4MUD6EYX6SFIHE77D2F");
//!     let protocol_config = Address::from_str(&env, "CC3OREX5QBIKJ5JOW36JFJJW7TLAKJOVT5WJXEITGALO7MU32KHICS2A");
//!
//!     client.initialize(&admin, &issuer_registry, &protocol_config);
//!
//!     // Create synthetic proof identifiers
//!     let proof_id_hash = BytesN::from_array(&env, &[10u8; 32]); // sha256("proof-id-456")
//!     let commitment_hash = BytesN::from_array(&env, &[11u8; 32]); // sha256(credential_payload)
//!     let issuer_address = Address::from_str(&env, "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U");
//!     let schema_version = 1u32;
//!     let current_timestamp = env.ledger().timestamp();
//!     let expires_at = current_timestamp + 86400u64; // 1 day in future
//!
//!     // Register the proof
//!     client.register_proof_with_type_identifier(
//!         &proof_id_hash,
//!         &commitment_hash,
//!         &issuer_address,
//!         &schema_version,
//!         &expires_at,
//!         &BytesN::from_array(&env, &[1u8; 32]),
//!     );
//!
//!     // Verify proof was registered with Active status
//!     let record = client.get_proof(&proof_id_hash);
//!     assert_eq!(record.proof_id_hash, proof_id_hash);
//!     assert_eq!(record.commitment_hash, commitment_hash);
//!     assert_eq!(record.issuer_address, issuer_address);
//!     assert_eq!(record.schema_version, schema_version);
//!     assert_eq!(record.status, ProofStatus::Active);
//!     assert_eq!(record.expires_at, expires_at);
//! }
//! ```
//!
//! ## Proof Lifecycle: Issuer Revocation
//!
//! Issuer revokes their own proof commitment:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, BytesN, Env};
//! use earnproof_shared::ProofStatus;
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! fn example_issuer_revoke_proof() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProofRegistryContract, ());
//!     let client = ProofRegistryContractClient::new(&env, &contract_id);
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer_registry = Address::from_str(&env, "CB73TVWVJIIVNTKLWSHZB5NL2UIF3B3EUL4YH4MUD6EYX6SFIHE77D2F");
//!     let protocol_config = Address::from_str(&env, "CC3OREX5QBIKJ5JOW36JFJJW7TLAKJOVT5WJXEITGALO7MU32KHICS2A");
//!
//!     client.initialize(&admin, &issuer_registry, &protocol_config);
//!
//!     let proof_id_hash = BytesN::from_array(&env, &[10u8; 32]);
//!     let commitment_hash = BytesN::from_array(&env, &[11u8; 32]);
//!     let issuer_address = Address::from_str(&env, "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U");
//!     let schema_version = 1u32;
//!     let current_timestamp = env.ledger().timestamp();
//!     let expires_at = current_timestamp + 86400u64;
//!
//!     // Register proof
//!     client.register_proof_with_type_identifier(
//!         &proof_id_hash,
//!         &commitment_hash,
//!         &issuer_address,
//!         &schema_version,
//!         &expires_at,
//!         &BytesN::from_array(&env, &[1u8; 32]),
//!     );
//!
//!     // Verify proof is valid
//!     assert!(client.is_valid_proof(&proof_id_hash));
//!     assert!(!client.is_revoked(&proof_id_hash));
//!
//!     // Issuer revokes their proof
//!     client.revoke_proof(&proof_id_hash);
//!
//!     // Verify proof is now revoked
//!     let record = client.get_proof(&proof_id_hash);
//!     assert_eq!(record.status, ProofStatus::Revoked);
//!     assert!(client.is_revoked(&proof_id_hash));
//!     assert!(!client.is_valid_proof(&proof_id_hash));
//! }
//! ```
//!
//! ## Proof Lifecycle: Admin Revocation
//!
//! Admin revokes a proof for compliance:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, BytesN, Env};
//! use earnproof_shared::ProofStatus;
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! fn example_admin_revoke_proof() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProofRegistryContract, ());
//!     let client = ProofRegistryContractClient::new(&env, &contract_id);
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer_registry = Address::from_str(&env, "CB73TVWVJIIVNTKLWSHZB5NL2UIF3B3EUL4YH4MUD6EYX6SFIHE77D2F");
//!     let protocol_config = Address::from_str(&env, "CC3OREX5QBIKJ5JOW36JFJJW7TLAKJOVT5WJXEITGALO7MU32KHICS2A");
//!
//!     client.initialize(&admin, &issuer_registry, &protocol_config);
//!
//!     let proof_id_hash = BytesN::from_array(&env, &[10u8; 32]);
//!     let commitment_hash = BytesN::from_array(&env, &[11u8; 32]);
//!     let issuer_address = Address::from_str(&env, "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U");
//!     let schema_version = 1u32;
//!     let current_timestamp = env.ledger().timestamp();
//!     let expires_at = current_timestamp + 86400u64;
//!
//!     client.register_proof_with_type_identifier(
//!         &proof_id_hash,
//!         &commitment_hash,
//!         &issuer_address,
//!         &schema_version,
//!         &expires_at,
//!         &BytesN::from_array(&env, &[1u8; 32]),
//!     );
//!
//!     // Admin revokes proof
//!     client.admin_revoke_proof(&proof_id_hash);
//!
//!     // Verify proof is revoked
//!     assert!(client.is_revoked(&proof_id_hash));
//! }
//! ```
//!
//! ## Proof Validity Checks
//!
//! Check proof validity status and expiration:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, BytesN, Env};
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! fn example_check_proof_validity() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProofRegistryContract, ());
//!     let client = ProofRegistryContractClient::new(&env, &contract_id);
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer_registry = Address::from_str(&env, "CB73TVWVJIIVNTKLWSHZB5NL2UIF3B3EUL4YH4MUD6EYX6SFIHE77D2F");
//!     let protocol_config = Address::from_str(&env, "CC3OREX5QBIKJ5JOW36JFJJW7TLAKJOVT5WJXEITGALO7MU32KHICS2A");
//!
//!     client.initialize(&admin, &issuer_registry, &protocol_config);
//!
//!     let proof_id_hash = BytesN::from_array(&env, &[10u8; 32]);
//!     let commitment_hash = BytesN::from_array(&env, &[11u8; 32]);
//!     let issuer_address = Address::from_str(&env, "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U");
//!     let schema_version = 1u32;
//!     let current_timestamp = env.ledger().timestamp();
//!     let expires_at = current_timestamp + 86400u64;
//!
//!     client.register_proof_with_type_identifier(
//!         &proof_id_hash,
//!         &commitment_hash,
//!         &issuer_address,
//!         &schema_version,
//!         &expires_at,
//!         &BytesN::from_array(&env, &[1u8; 32]),
//!     );
//!
//!     // Proof is valid (Active status and not expired)
//!     assert!(client.is_valid_proof(&proof_id_hash));
//!     assert!(!client.is_revoked(&proof_id_hash));
//!
//!     // Get full record for detailed inspection
//!     let record = client.get_proof(&proof_id_hash);
//!     assert_eq!(record.commitment_hash, commitment_hash);
//!     assert_eq!(record.schema_version, schema_version);
//!     assert_eq!(record.expires_at, expires_at);
//! }
//! ```
