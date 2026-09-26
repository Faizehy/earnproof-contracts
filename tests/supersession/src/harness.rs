
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env,
};

pub const APPROVED_SCHEMA: u32 = 1;

/// Deterministic 32-byte value derived from one discriminator byte.
fn hash(env: &Env, discriminator: u8) -> BytesN<32> {
    BytesN::from_array(env, &[discriminator; 32])
}

pub struct Deployment {
    pub env: Env,
    pub config: ProtocolConfigContractClient<'static>,
    pub issuers: IssuerRegistryContractClient<'static>,
    pub proofs: ProofRegistryContractClient<'static>,
    pub admin: Address,
    pub issuer: Address,
    pub issuer_id: BytesN<32>,
}

impl Deployment {
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let admin = Address::generate(&env);
        let issuer = Address::generate(&env);

        let config_id = env.register(ProtocolConfigContract, ());
        let config = ProtocolConfigContractClient::new(&env, &config_id);
        config.initialize(&admin);
        config.approve_schema_version(&APPROVED_SCHEMA);

        let issuers_id = env.register(IssuerRegistryContract, ());
        let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
        issuers.initialize(&admin);
        let issuer_id = hash(&env, 0x01);
        issuers.register_issuer(&issuer_id, &issuer, &hash(&env, 0xAA), &hash(&env, 0x99));

        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
        proofs.initialize(&admin, &issuers.address, &config.address);

        Self {
            env,
            config,
            issuers,
            proofs,
            admin,
            issuer,
            issuer_id,
        }
    }

    pub fn register_proof(
        &self,
        proof_id: &BytesN<32>,
        predecessor: Option<BytesN<32>>,
    ) -> BytesN<32> {
        let commitment = hash(&self.env, 0xFF);
        let expires_at = self.env.ledger().timestamp() + 100_000;
        self.proofs.register_proof(
            proof_id,
            &commitment,
            &self.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
            &predecessor,
        );
        proof_id.clone()
    }
}
