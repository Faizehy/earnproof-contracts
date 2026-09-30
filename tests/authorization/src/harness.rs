//! Shared fixtures for the authorization negative matrix.
//!
//! Unlike the emergency harness, this harness does **not** call
//! `mock_all_auths`. Every privileged call is authorized explicitly through
//! [`authorize`], which installs a *matching-mode* auth entry: the host admits
//! the next invocation only when the demanded signer, function, and arguments
//! match the entry exactly, and rejects everything else. That enforcement is
//! what lets the matrix assert both the returned error and the absence of side
//! effects for missing and wrong identities.
//!
//! The deployment is the full three-contract set so that cross-contract
//! authorization boundaries (`proof-registry`'s issuer-vs-admin revocation
//! paths, independent per-contract admins) can be exercised from one place.

use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, BytesN, Env, IntoVal, Map, Val};

use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};

/// Schema version approved by [`Deployment::new`] and used by the fixture
/// proofs.
pub const APPROVED_SCHEMA: u32 = 1;

/// Installs a matching-mode auth entry: `signer` is authorized for exactly one
/// invocation of `fn_name` on `contract` with `args`. Any other signer,
/// function, or argument set is rejected by the host.
pub fn authorize(
    env: &Env,
    signer: &Address,
    contract: &Address,
    fn_name: &str,
    args: soroban_sdk::Vec<Val>,
) {
    env.mock_auths(&[MockAuth {
        address: signer,
        invoke: &MockAuthInvoke {
            contract,
            fn_name,
            args,
            sub_invokes: &[],
        },
    }]);
}

/// A fully wired deployment with real auth enforcement.
pub struct Deployment<'a> {
    pub env: Env,
    pub config: ProtocolConfigContractClient<'a>,
    pub issuers: IssuerRegistryContractClient<'a>,
    pub proofs: ProofRegistryContractClient<'a>,
    pub config_address: Address,
    pub issuers_address: Address,
    pub proofs_address: Address,
    /// Administrator of all three contracts at deployment time.
    pub admin: Address,
    /// An active issuer, authorized to register proofs.
    pub issuer: Address,
    /// A second active issuer, used as the "wrong but privileged" signer in
    /// proof-registry rows: it is not the named issuer, so its signature must
    /// not be accepted.
    pub second_issuer: Address,
    pub issuer_id: BytesN<32>,
    pub second_issuer_id: BytesN<32>,
}

impl Deployment<'_> {
    /// Deploys and fully initializes the three-contract set. `initialize`,
    /// `approve_schema_version`, and `register_issuer` are all authorized as
    /// `admin`; `second_issuer` is registered as a second active issuer.
    pub fn new() -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);

        let admin = Address::generate(&env);
        let issuer = Address::generate(&env);
        let second_issuer = Address::generate(&env);

        let config_id = env.register(ProtocolConfigContract, ());
        let config = ProtocolConfigContractClient::new(&env, &config_id);
        let issuers_id = env.register(IssuerRegistryContract, ());
        let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);

        // protocol-config: initialize + approve the fixture schema.
        authorize(
            &env,
            &admin,
            &config_id,
            "initialize",
            (&admin,).into_val(&env),
        );
        config.initialize(&admin);
        authorize(
            &env,
            &admin,
            &config_id,
            "approve_schema_version",
            (&APPROVED_SCHEMA,).into_val(&env),
        );
        config.approve_schema_version(&APPROVED_SCHEMA);

        // issuer-registry: initialize + register two active issuers.
        let issuer_id = issuer_id_hash(&env, 1);
        let second_issuer_id = issuer_id_hash(&env, 2);
        authorize(
            &env,
            &admin,
            &issuers_id,
            "initialize",
            (&admin,).into_val(&env),
        );
        issuers.initialize(&admin);
        authorize(
            &env,
            &admin,
            &issuers_id,
            "register_issuer",
            (&issuer_id, &issuer, &hash(&env, 0xAA), &hash(&env, 0x99)).into_val(&env),
        );
        issuers.register_issuer(&issuer_id, &issuer, &hash(&env, 0xAA), &hash(&env, 0x99));
        authorize(
            &env,
            &admin,
            &issuers_id,
            "register_issuer",
            (
                &second_issuer_id,
                &second_issuer,
                &hash(&env, 0xBB),
                &hash(&env, 0x99),
            )
                .into_val(&env),
        );
        issuers.register_issuer(
            &second_issuer_id,
            &second_issuer,
            &hash(&env, 0xBB),
            &hash(&env, 0x99),
        );

        // proof-registry: initialize with the two supporting contracts.
        authorize(
            &env,
            &admin,
            &proofs_id,
            "initialize",
            (&admin, &issuers_id, &config_id).into_val(&env),
        );
        proofs.initialize(&admin, &issuers_id, &config_id);

        Self {
            env,
            config,
            issuers,
            proofs,
            config_address: config_id,
            issuers_address: issuers_id,
            proofs_address: proofs_id,
            admin,
            issuer,
            second_issuer,
            issuer_id,
            second_issuer_id,
        }
    }

    /// A fresh, unrelated address that holds no authority anywhere. Generated
    /// on demand so every negative attempt uses a distinct identity.
    pub fn attacker(&self) -> Address {
        Address::generate(&self.env)
    }

    /// Deploys the three contracts without initializing any of them. Used by
    /// the `initialize` rows of the matrix, where the deployment must still be
    /// uninitialized when the attempt runs.
    pub fn uninitialized() -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);

        let admin = Address::generate(&env);
        let issuer = Address::generate(&env);
        let second_issuer = Address::generate(&env);

        let config_id = env.register(ProtocolConfigContract, ());
        let config = ProtocolConfigContractClient::new(&env, &config_id);
        let issuers_id = env.register(IssuerRegistryContract, ());
        let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
        let issuer_id = issuer_id_hash(&env, 1);
        let second_issuer_id = issuer_id_hash(&env, 2);

        Self {
            env,
            config,
            issuers,
            proofs,
            config_address: config_id,
            issuers_address: issuers_id,
            proofs_address: proofs_id,
            admin,
            issuer,
            second_issuer,
            issuer_id,
            second_issuer_id,
        }
    }

    /// Authorized wrapper around the privileged operations the fixture and the
    /// rotation scenarios need. Each method installs the auth entry for the
    /// signer the contract is documented to demand.
    pub fn set_admin(&self, new_admin: &Address) {
        authorize(
            &self.env,
            &self.admin,
            &self.config_address,
            "nominate_admin",
            (new_admin,).into_val(&self.env),
        );
        self.config.nominate_admin(new_admin);
        authorize(
            &self.env,
            new_admin,
            &self.config_address,
            "accept_admin",
            ().into_val(&self.env),
        );
        self.config.accept_admin();
    }

    pub fn suspend_issuer(&self, issuer_id: &BytesN<32>) {
        authorize(
            &self.env,
            &self.admin,
            &self.issuers_address,
            "suspend_issuer",
            (issuer_id, &soroban_sdk::BytesN::from_array(&self.env, &[1u8; 32])).into_val(&self.env),
        );
        self.issuers.suspend_issuer(
            issuer_id,
            &soroban_sdk::BytesN::from_array(&self.env, &[1u8; 32]),
        );
    }

    pub fn rotate_issuer_address(&self, issuer_id: &BytesN<32>, new_address: &Address) {
        authorize(
            &self.env,
            &self.admin,
            &self.issuers_address,
            "rotate_issuer_address",
            (issuer_id, new_address).into_val(&self.env),
        );
        self.issuers.rotate_issuer_address(issuer_id, new_address);
    }

    /// Registers a proof with the given discriminator as `issuer` and returns
    /// its id hash. Fails the test if registration is rejected.
    pub fn register_proof(&self, discriminator: u8) -> BytesN<32> {
        let proof_id = hash(&self.env, discriminator);
        let commitment = hash(&self.env, discriminator ^ 0xFF);
        let expires_at = self.env.ledger().timestamp() + 100_000;
        authorize(
            &self.env,
            &self.issuer,
            &self.proofs_address,
            "register_proof",
            (
                &proof_id,
                &commitment,
                &self.issuer,
                &APPROVED_SCHEMA,
                &expires_at,
            )
                .into_val(&self.env),
        );
        self.proofs.register_proof(
            &proof_id,
            &commitment,
            &self.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
        );
        proof_id
    }
}

// ---------------------------------------------------------------------------
// Snapshots
//
// A rejected call must leave the entire observable surface untouched: storage,
// storage TTLs, instance TTLs, and events. The snapshot captures instance
// storage per contract and all persistent storage globally.
//
// NOTE: Soroban SDK's `Persistent::all()` does not filter by contract
// address — it returns persistent entries from every contract in the test
// environment. We therefore capture persistent storage as a single global
// map at the Env level (not using `as_contract`), and compare that map
// before and after the rejected call. Instance storage is safe to capture
// per contract because `Instance::all()` correctly filters by address.
// ---------------------------------------------------------------------------

/// Instance storage and TTL state of one contract.
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceSnapshot {
    pub address: Address,
    pub instance: Map<Val, Val>,
    pub instance_ttl: u32,
}

/// Observable state of the whole deployment.
#[derive(Clone, Debug, PartialEq)]
pub struct DeploymentSnapshot {
    pub config: InstanceSnapshot,
    pub issuers: InstanceSnapshot,
    pub proofs: InstanceSnapshot,
    /// All persistent storage entries across every contract, captured at the
    /// Env level so contract-boundary filtering issues do not arise.
    pub persistent: Map<Val, Val>,
}

impl Deployment<'_> {
    /// Captures instance storage for a single contract.
    fn capture_instance(&self, address: &Address) -> InstanceSnapshot {
        self.env.as_contract(address, || {
            use soroban_sdk::testutils::storage::Instance as _;
            let instance = self.env.storage().instance().all();
            let instance_ttl = self.env.storage().instance().get_ttl();
            InstanceSnapshot {
                address: address.clone(),
                instance,
                instance_ttl,
            }
        })
    }

    /// Captures the state of all three contracts. Persistent storage is
    /// captured inside `as_contract` (the SDK requires it), and the result
    /// is the same regardless of which contract context is used because
    /// `Persistent::all()` returns entries from every contract.
    pub fn snapshot(&self) -> DeploymentSnapshot {
        use soroban_sdk::testutils::storage::Persistent as _;
        let persistent = self.env.as_contract(&self.config_address, || {
            self.env.storage().persistent().all()
        });
        DeploymentSnapshot {
            config: self.capture_instance(&self.config_address),
            issuers: self.capture_instance(&self.issuers_address),
            proofs: self.capture_instance(&self.proofs_address),
            persistent,
        }
    }

    /// Asserts that a rejected call left every observable surface untouched.
    pub fn assert_no_side_effects(&self, before: &DeploymentSnapshot, label: &str) {
        let after = self.snapshot();
        assert_eq!(
            after, *before,
            "{label}: a rejected call changed storage state"
        );
        assert!(
            self.env.events().all().events().is_empty(),
            "{label}: a rejected call emitted events"
        );
    }
}

/// Deterministic 32-byte value derived from a single discriminator byte.
pub fn hash(env: &Env, discriminator: u8) -> BytesN<32> {
    BytesN::from_array(env, &[discriminator; 32])
}

/// Issuer id hashes live in a disjoint range from proof id hashes so that a
/// scenario mixing the two cannot accidentally collide.
pub fn issuer_id_hash(env: &Env, discriminator: u8) -> BytesN<32> {
    hash(env, 0x80 | discriminator)
}
