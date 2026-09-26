//! Protocol Config Contract Invocation Examples
//!
//! This module contains executable documentation examples demonstrating
//! how to invoke the ProtocolConfig contract methods. All examples use
//! synthetic identifiers and run in a local sandbox environment.
//!
//! # Examples
//!
//! ## Initialization
//!
//! Initialize the protocol config contract with an admin address:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, Env};
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//!
//! # #[test]
//! fn example_initialize_protocol() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProtocolConfigContract, ());
//!     let client = ProtocolConfigContractClient::new(&env, &contract_id);
//!
//!     // Use a synthetic admin address for "sandbox" network
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!
//!     // Initialize the protocol config
//!     client.initialize(&admin);
//!
//!     // Verify admin is set
//!     assert_eq!(client.get_admin(), admin);
//!     assert!(!client.is_paused());
//!     assert_eq!(client.get_config_version(), 1);
//! }
//! ```
//!
//! ## Approve Schema Version
//!
//! Approve a schema version for proof registration:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, Env};
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//!
//! # #[test]
//! fn example_approve_schema() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProtocolConfigContract, ());
//!     let client = ProtocolConfigContractClient::new(&env, &contract_id);
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     client.initialize(&admin);
//!
//!     // Approve schema version 1
//!     client.approve_schema_version(&1);
//!     assert!(client.is_schema_version_approved(&1));
//!
//!     // Config version increments after each state change
//!     assert_eq!(client.get_config_version(), 2);
//! }
//! ```
//!
//! ## Pause Protocol
//!
//! Pause the protocol to block proof registration:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, Env};
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//!
//! # #[test]
//! fn example_pause_protocol() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProtocolConfigContract, ());
//!     let client = ProtocolConfigContractClient::new(&env, &contract_id);
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     client.initialize(&admin);
//!
//!     // Pause the protocol
//!     client.pause();
//!     assert!(client.is_paused());
//!
//!     // Unpause the protocol
//!     client.unpause();
//!     assert!(!client.is_paused());
//! }
//! ```
//!
//! ## Change Admin
//!
//! Transfer admin responsibilities to a new address:
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, Env};
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//!
//! # #[test]
//! fn example_set_admin() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProtocolConfigContract, ());
//!     let client = ProtocolConfigContractClient::new(&env, &contract_id);
//!
//!     let admin_1 = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     let admin_2 = Address::from_str(&env, "GDWUSKGGFDI4FRXK5EBTRECZSVQSSWJHHJOGH6JWG3AUMFFMQ435DIAG");
//!
//!     client.initialize(&admin_1);
//!     assert_eq!(client.get_admin(), admin_1);
//!
//!     // Transfer admin to admin_2
//!     client.nominate_admin(&admin_2);
        client.accept_admin();
//!     assert_eq!(client.get_admin(), admin_2);
//! }
//! ```
//!
//! ## Deprecate Schema Version
//!
//! Mark a schema version as deprecated (does not delete it):
//!
//! ```no_run
//! # extern crate std;
//! use soroban_sdk::{Address, Env};
//! use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
//!
//! # #[test]
//! fn example_deprecate_schema() {
//!     let env = Env::default();
//!     env.mock_all_auths();
//!
//!     let contract_id = env.register(ProtocolConfigContract, ());
//!     let client = ProtocolConfigContractClient::new(&env, &contract_id);
//!
//!     let admin = Address::from_str(&env, "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR");
//!     client.initialize(&admin);
//!
//!     // Approve version 1
//!     client.approve_schema_version(&1);
//!     assert!(client.is_schema_version_approved(&1));
//!
//!     // Deprecate version 1
//!     client.deprecate_schema_version(&1);
//!     assert!(!client.is_schema_version_approved(&1));
//! }
//! ```
