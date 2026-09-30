//! Shared fixtures for the emergency scenarios.
//!
//! The harness deploys the full three-contract set so that pause behaviour can
//! be observed through the contracts that consume it, not just through
//! `protocol-config` in isolation.

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env};

use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};

/// Schema version approved by [`Deployment::new`]. Registration is expected to
/// succeed for this version whenever the protocol is unpaused.
pub const APPROVED_SCHEMA: u32 = 1;

/// A fully wired deployment: protocol config, issuer registry, proof registry.
pub struct Deployment<'a> {
    pub env: Env,
    pub config: ProtocolConfigContractClient<'a>,
    pub issuers: IssuerRegistryContractClient<'a>,
    pub proofs: ProofRegistryContractClient<'a>,
    /// Administrator of all three contracts at deployment time.
    pub admin: Address,
    /// An active issuer, authorised to register proofs.
    pub issuer: Address,
}

impl Deployment<'_> {
    /// Deploys the three contracts, registers one active issuer, and approves
    /// [`APPROVED_SCHEMA`]. All auth is mocked; scenarios that care about
    /// authorisation assert on it explicitly via [`Deployment::assert_admin_only`].
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let admin = Address::generate(&env);
        let issuer = Address::generate(&env);

        let config_id = env.register(ProtocolConfigContract, ());
        let config = ProtocolConfigContractClient::new(&env, &config_id);
        config.initialize(&admin);
        config.approve_schema_version(&APPROVED_SCHEMA);

        let issuers_id = env.register(IssuerRegistryContract, ());
        let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
        issuers.initialize(&admin);
        issuers.register_issuer(
            &issuer_id_hash(&env, 1),
            &issuer,
            &hash(&env, 0xAA),
            &hash(&env, 0x99),
        );

        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
        proofs.initialize(&admin, &issuers_id, &config_id);

        Self {
            env,
            config,
            issuers,
            proofs,
            admin,
            issuer,
        }
    }

    /// Registers a proof with the given discriminator and returns its id hash.
    /// Fails the test if registration is rejected.
    pub fn register_proof(&self, discriminator: u8) -> BytesN<32> {
        let proof_id = hash(&self.env, discriminator);
        self.proofs.register_proof(
            &proof_id,
            &hash(&self.env, discriminator ^ 0xFF),
            &self.issuer,
            &APPROVED_SCHEMA,
            &(self.env.ledger().timestamp() + 100_000),
        );
        proof_id
    }

    /// Advances the ledger clock, used to expire proofs.
    pub fn advance(&self, seconds: u64) {
        let now = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(now + seconds);
    }
}

/// Deterministic 32-byte value derived from a single discriminator byte.
///
/// Using a derived constant rather than a random or real-world value keeps the
/// fixtures free of anything resembling a production proof identifier.
pub fn hash(env: &Env, discriminator: u8) -> BytesN<32> {
    BytesN::from_array(env, &[discriminator; 32])
}

/// Issuer id hashes live in a disjoint range from proof id hashes so that a
/// scenario mixing the two cannot accidentally collide.
pub fn issuer_id_hash(env: &Env, discriminator: u8) -> BytesN<32> {
    hash(env, 0x80 | discriminator)
}
