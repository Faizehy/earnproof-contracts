//! Cross-Contract Integration Examples
//!
//! This module contains executable documentation examples demonstrating
//! how the three contracts interact together in real-world workflows.
//!
//! # Examples
//!
//! ## End-to-End Workflow: Register Issuer and Proof
//!
//! Complete workflow: initialize all contracts, register an issuer,
//! approve a schema, and register a proof:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, BytesN, Env};
//! use earnproof_shared::ProofStatus;
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//! use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! fn example_end_to_end_workflow() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     // Setup admin and addresses
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer = Address::from_str(&env, "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U");
//!
//!     // Initialize Protocol Config
//!     let protocol_id = env.register(ProtocolConfigContract, ());
//!     let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_id);
//!     protocol_client.initialize(&admin);
//!
//!     // Approve schema version 1
//!     protocol_client.approve_schema_version(&1);
//!     protocol_client.approve_proof_type(&BytesN::from_array(&env, &[1u8; 32]));
//!     assert!(protocol_client.is_schema_version_approved(&1));
//!
//!     // Initialize Issuer Registry
//!     let issuer_registry_id = env.register(IssuerRegistryContract, ());
//!     let issuer_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
//!     issuer_client.initialize(&admin);
//!
//!     // Register an issuer
//!     let issuer_id_hash = BytesN::from_array(&env, &[1u8; 32]);
//!     let metadata_hash = BytesN::from_array(&env, &[2u8; 32]);
//!     issuer_client.register_issuer(&issuer_id_hash, &issuer, &metadata_hash, &metadata_hash);
//!     assert!(issuer_client.is_active_address(&issuer));
//!
//!     // Initialize Proof Registry with cross-contract references
//!     let proof_registry_id = env.register(ProofRegistryContract, ());
//!     let proof_client = ProofRegistryContractClient::new(&env, &proof_registry_id);
//!     proof_client.initialize(&admin, &issuer_registry_id, &protocol_id);
//!
//!     // Register a proof (requires all upstream checks)
//!     let proof_id_hash = BytesN::from_array(&env, &[10u8; 32]);
//!     let commitment_hash = BytesN::from_array(&env, &[11u8; 32]);
//!     let current_timestamp = env.ledger().timestamp();
//!     let expires_at = current_timestamp + 86400u64;
//!
//!     proof_client.register_proof(
//!         &proof_id_hash,
//!         &commitment_hash,
//!         &issuer,
//!         &1,
//!         &expires_at,
//!         &BytesN::from_array(&env, &[1u8; 32]),
//!     );
//!
//!     // Verify complete workflow succeeded
//!     assert!(proof_client.is_valid_proof(&proof_id_hash));
//!     let record = proof_client.get_proof(&proof_id_hash);
//!     assert_eq!(record.status, ProofStatus::Active);
//! }
//! ```
//!
//! ## Error Case: Proof Registration Blocked by Paused Protocol
//!
//! Demonstrate that proof registration fails when protocol is paused:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, BytesN, Env};
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//! use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! #[should_panic(expected = "protocol is paused")]
//! fn example_proof_registration_blocked_by_pause() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer = Address::from_str(&env, "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U");
//!
//!     // Initialize and setup
//!     let protocol_id = env.register(ProtocolConfigContract, ());
//!     let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_id);
//!     protocol_client.initialize(&admin);
//!     protocol_client.approve_schema_version(&1);
//!     protocol_client.approve_proof_type(&BytesN::from_array(&env, &[1u8; 32]));
//!
//!     let issuer_registry_id = env.register(IssuerRegistryContract, ());
//!     let issuer_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
//!     issuer_client.initialize(&admin);
//!
//!     let issuer_id_hash = BytesN::from_array(&env, &[1u8; 32]);
//!     issuer_client.register_issuer(&issuer_id_hash, &issuer, &BytesN::from_array(&env, &[2u8; 32]));
//!
//!     let proof_registry_id = env.register(ProofRegistryContract, ());
//!     let proof_client = ProofRegistryContractClient::new(&env, &proof_registry_id);
//!     proof_client.initialize(&admin, &issuer_registry_id, &protocol_id);
//!
//!     // Pause the protocol
//!     protocol_client.pause();
//!     assert!(protocol_client.is_paused());
//!
//!     // This call should panic with "protocol is paused"
//!     let proof_id_hash = BytesN::from_array(&env, &[10u8; 32]);
//!     let commitment_hash = BytesN::from_array(&env, &[11u8; 32]);
//!     let current_timestamp = env.ledger().timestamp();
//!     let expires_at = current_timestamp + 86400u64;
//!
//!     proof_client.register_proof(
//!         &proof_id_hash,
//!         &commitment_hash,
//!         &issuer,
//!         &1,
//!         &expires_at,
//!         &BytesN::from_array(&env, &[1u8; 32]),
//!     );
//! }
//! ```
//!
//! ## Error Case: Proof Registration Blocked by Inactive Issuer
//!
//! Demonstrate that proof registration fails when issuer is suspended:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, BytesN, Env};
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//! use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! #[should_panic(expected = "issuer is not active")]
//! fn example_proof_registration_blocked_by_suspended_issuer() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer = Address::from_str(&env, "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U");
//!
//!     // Initialize all contracts
//!     let protocol_id = env.register(ProtocolConfigContract, ());
//!     let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_id);
//!     protocol_client.initialize(&admin);
//!     protocol_client.approve_schema_version(&1);
//!     protocol_client.approve_proof_type(&BytesN::from_array(&env, &[1u8; 32]));
//!
//!     let issuer_registry_id = env.register(IssuerRegistryContract, ());
//!     let issuer_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
//!     issuer_client.initialize(&admin);
//!
//!     let issuer_id_hash = BytesN::from_array(&env, &[1u8; 32]);
//!     issuer_client.register_issuer(&issuer_id_hash, &issuer, &BytesN::from_array(&env, &[2u8; 32]));
//!
//!     let proof_registry_id = env.register(ProofRegistryContract, ());
//!     let proof_client = ProofRegistryContractClient::new(&env, &proof_registry_id);
//!     proof_client.initialize(&admin, &issuer_registry_id, &protocol_id);
//!
//!     // Suspend the issuer
//!     issuer_client.suspend_issuer(&issuer_id_hash);
//!     assert!(!issuer_client.is_active_address(&issuer));
//!
//!     // This call should panic with "issuer is not active"
//!     let proof_id_hash = BytesN::from_array(&env, &[10u8; 32]);
//!     let commitment_hash = BytesN::from_array(&env, &[11u8; 32]);
//!     let current_timestamp = env.ledger().timestamp();
//!     let expires_at = current_timestamp + 86400u64;
//!
//!     proof_client.register_proof(
//!         &proof_id_hash,
//!         &commitment_hash,
//!         &issuer,
//!         &1,
//!         &expires_at,
//!         &BytesN::from_array(&env, &[1u8; 32]),
//!     );
//! }
//! ```
//!
//! ## Error Case: Proof Registration Blocked by Unapproved Schema
//!
//! Demonstrate that proof registration fails for unapproved schema versions:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, BytesN, Env};
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//! use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
//! use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
//!
//! # #[test]
//! #[should_panic(expected = "schema version is not approved")]
//! fn example_proof_registration_blocked_by_unapproved_schema() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let issuer = Address::from_str(&env, "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U");
//!
//!     let protocol_id = env.register(ProtocolConfigContract, ());
//!     let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_id);
//!     protocol_client.initialize(&admin);
//!     // Only approve version 1, not version 2
//!     protocol_client.approve_schema_version(&1);
//!     protocol_client.approve_proof_type(&BytesN::from_array(&env, &[1u8; 32]));
//!
//!     let issuer_registry_id = env.register(IssuerRegistryContract, ());
//!     let issuer_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
//!     issuer_client.initialize(&admin);
//!
//!     let issuer_id_hash = BytesN::from_array(&env, &[1u8; 32]);
//!     issuer_client.register_issuer(&issuer_id_hash, &issuer, &BytesN::from_array(&env, &[2u8; 32]), &BytesN::from_array(&env, &[3u8; 32]));
//!
//!     let proof_registry_id = env.register(ProofRegistryContract, ());
//!     let proof_client = ProofRegistryContractClient::new(&env, &proof_registry_id);
//!     proof_client.initialize(&admin, &issuer_registry_id, &protocol_id);
//!
//!     // Try to register proof with unapproved schema version 2
//!     let proof_id_hash = BytesN::from_array(&env, &[10u8; 32]);
//!     let commitment_hash = BytesN::from_array(&env, &[11u8; 32]);
//!     let current_timestamp = env.ledger().timestamp();
//!     let expires_at = current_timestamp + 86400u64;
//!
//!     proof_client.register_proof(
//!         &proof_id_hash,
//!         &commitment_hash,
//!         &issuer,
//!         &2, // Unapproved schema version
//!         &expires_at,
//!         &BytesN::from_array(&env, &[1u8; 32]),
//!     );
//! }
//! ```
