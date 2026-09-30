//! Shared deployment fixture and storage-key helpers.
//!
//! Storage keys are rebuilt here as tuples rather than imported, because each
//! contract keeps its `DataKey` enum private. A `#[contracttype]` enum variant
//! and the equivalent Rust tuple encode to the same host value, so the tuples
//! below address exactly the same ledger entries the contracts write. If a
//! contract ever renames a variant, the corresponding test fails on a missing
//! key, which is the intended alarm.

use earnproof_shared::{TTL_EXTEND_TO_LEDGERS, TTL_THRESHOLD_LEDGERS};
use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol};

/// Ledger sequence every fixture starts from. Chosen to be non-zero so that a
/// test can never accidentally pass because of an underflow to sequence 0.
pub const GENESIS_SEQUENCE: u32 = 1_000;

/// Ledger timestamp every fixture starts from.
pub const GENESIS_TIMESTAMP: u64 = 1_000;

/// Expiration timestamp used by proofs that must stay valid for the whole test.
pub const FAR_FUTURE: u64 = 10_000_000;

/// The schema version approved by the fixture.
pub const SCHEMA_VERSION: u32 = 1;

pub const PROOF_ID: u8 = 5;
pub const COMMITMENT: u8 = 6;
pub const ISSUER_ID: u8 = 1;
pub const ISSUER_METADATA: u8 = 2;

pub struct Deployment {
    pub env: Env,
    pub proofs: ProofRegistryContractClient<'static>,
    pub issuers: IssuerRegistryContractClient<'static>,
    pub config: ProtocolConfigContractClient<'static>,
    pub proofs_id: Address,
    pub issuers_id: Address,
    pub config_id: Address,
    pub admin: Address,
    pub issuer: Address,
}

impl Deployment {
    /// Advances the ledger sequence by `ledgers` without making any contract
    /// call, which is what an idle period on the network looks like.
    pub fn idle(&self, ledgers: u32) {
        let current = self.env.ledger().sequence();
        self.env.ledger().set_sequence_number(current + ledgers);
    }

    /// Registers the fixture proof and returns its identifier.
    pub fn register_proof(&self, expires_at: u64) -> BytesN<32> {
        let proof_id = bytes(&self.env, PROOF_ID);
        self.proofs.register_proof(
            &proof_id,
            &bytes(&self.env, COMMITMENT),
            &self.issuer,
            &SCHEMA_VERSION,
            &expires_at,
        );
        proof_id
    }

    /// Reads the TTL of the fixture proof entry.
    ///
    /// This is a storage access like any other, so on an archived entry it
    /// triggers the same auto-restoration a contract call would.
    pub fn proof_ttl(&self, proof_id: &BytesN<32>) -> u32 {
        let key = proof_key(&self.env, proof_id);
        self.env.as_contract(&self.proofs_id, || {
            use soroban_sdk::testutils::storage::Persistent as _;
            self.env.storage().persistent().get_ttl(&key)
        })
    }

    /// Reads the proof registry's instance TTL.
    pub fn proof_instance_ttl(&self) -> u32 {
        self.env.as_contract(&self.proofs_id, || {
            use soroban_sdk::testutils::storage::Instance as _;
            self.env.storage().instance().get_ttl()
        })
    }

    /// Number of entries the given contract holds in temporary storage.
    pub fn temporary_entry_count(&self, contract: &Address) -> u32 {
        self.env.as_contract(contract, || {
            use soroban_sdk::testutils::storage::Temporary as _;
            self.env.storage().temporary().all().len()
        })
    }
}

pub fn bytes(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}

/// `proof-registry::DataKey::Proof(proof_id_hash)`.
pub fn proof_key(env: &Env, proof_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    let _ = env;
    (symbol_short!("Proof"), proof_id.clone())
}

/// `issuer-registry::DataKey::Issuer(issuer_id_hash)`.
pub fn issuer_key(env: &Env, issuer_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    let _ = env;
    (symbol_short!("Issuer"), issuer_id.clone())
}

/// `issuer-registry::DataKey::AddressIssuer(issuer_address)`.
pub fn address_issuer_key(env: &Env, address: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "AddressIssuer"), address.clone())
}

/// `protocol-config::DataKey::SchemaVersion(version)`.
pub fn schema_version_key(env: &Env, version: u32) -> (Symbol, u32) {
    (Symbol::new(env, "SchemaVersion"), version)
}

/// Deploys and initializes all three contracts at [`GENESIS_SEQUENCE`].
pub fn deployment() -> Deployment {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(GENESIS_SEQUENCE);
    env.ledger().set_timestamp(GENESIS_TIMESTAMP);

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);

    let config_id = env.register(ProtocolConfigContract, ());
    let config = ProtocolConfigContractClient::new(&env, &config_id);
    config.initialize(&admin);
    config.approve_schema_version(&SCHEMA_VERSION);

    let issuers_id = env.register(IssuerRegistryContract, ());
    let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
    issuers.initialize(&admin);
    issuers.register_issuer(
        &bytes(&env, ISSUER_ID),
        &issuer,
        &bytes(&env, ISSUER_METADATA),
        &bytes(&env, 99),
    );

    let proofs_id = env.register(ProofRegistryContract, ());
    let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
    proofs.initialize(&admin, &issuers_id, &config_id);

    Deployment {
        env,
        proofs,
        issuers,
        config,
        proofs_id,
        issuers_id,
        config_id,
        admin,
        issuer,
    }
}

/// Sanity guard: the constants the whole suite is written against. A threshold
/// above the target would be rejected by the host on every extension call, so
/// the ordering is checked at compile time.
const _: () = assert!(TTL_THRESHOLD_LEDGERS < TTL_EXTEND_TO_LEDGERS);

#[test]
fn ttl_constants_are_the_documented_pair() {
    assert_eq!(TTL_THRESHOLD_LEDGERS, 50_000);
    assert_eq!(TTL_EXTEND_TO_LEDGERS, 500_000);
}

/// The minimum persistent TTL the host applies on auto-restoration. Several
/// assertions depend on it, so it is pinned here rather than hard-coded.
#[test]
fn host_minimum_persistent_ttl_is_the_value_tests_assume() {
    let deployment = deployment();
    assert_eq!(
        deployment.env.ledger().get().min_persistent_entry_ttl,
        4_096
    );
}
