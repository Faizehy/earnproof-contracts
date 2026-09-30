//! Fixtures, rejection classification, and the state footprint the scenarios
//! assert on.
//!
//! # The registration path
//!
//! `register_proof` performs these steps, in this order. The numbered rows are
//! the cross-contract read boundaries; everything else is local to
//! `proof-registry`.
//!
//! | Step | What happens                                            |
//! |------|---------------------------------------------------------|
//! | A    | `issuer_address.require_auth()`                         |
//! | B    | reject `schema_version == 0`                            |
//! | C    | reject `expires_at <= now`                              |
//! | D    | read the `ProtocolConfig` reference from instance storage |
//! | **1**| `protocol-config::is_paused() -> bool`                   |
//! | **2**| `protocol-config::is_schema_version_approved(u32) -> bool` |
//! | E    | read the `IssuerRegistry` reference from instance storage |
//! | **3**| `issuer-registry::is_active_address(Address) -> bool`    |
//! | F    | reject a duplicate `Proof(proof_id_hash)` key            |
//! | G    | write the record and extend its TTL                     |
//!
//! Steps A–C give "before the first read", boundaries 1–3 give "during a
//! read", and E–G give "after a read". [`crate::mocks`] supplies substitute
//! dependencies that fail at a chosen boundary, which is what makes the
//! ordering above observable from outside rather than merely asserted.
//!
//! # Reading state without disturbing it
//!
//! Almost every public read in this workspace extends a TTL — `get_proof`,
//! `get_issuer`, and `is_schema_version_approved` all do. A snapshot built from
//! those getters could not distinguish "the failed call extended the TTL" from
//! "the snapshot did". [`Deployment::footprint`] therefore reads the entries
//! whose TTL it asserts on straight out of storage through
//! [`soroban_sdk::Env::as_contract`], which extends nothing.

use earnproof_shared::{IssuerRecord, ProofError, ProofRecord};
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env, InvokeError, Symbol};

use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};

/// Schema version approved by [`Deployment::with_dependency_addresses`].
pub const APPROVED_SCHEMA: u32 = 1;

/// Ledger timestamp every deployment starts at. Fixed rather than default so
/// that "expires in the past" is expressible without underflow.
pub const START_TIMESTAMP: u64 = 1_000;

/// How far in the future a registration's expiry is placed by default.
pub const PROOF_LIFETIME: u64 = 100_000;

/// How an invocation ended, expressed without reference to any panic text.
///
/// `docs/compatibility.md` classifies panic messages as *not* a stable
/// interface: "a cross-contract rejection surfaces as `Error(WasmVm,
/// InvalidAction)` and the message reaches the caller only through the
/// diagnostic log. Consumers must not match on them." These tests hold
/// themselves to the same rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    /// The invocation succeeded.
    Accepted,
    /// `proof-registry` returned one of its own documented `ProofError` codes.
    /// This is the surface `docs/backend-integration.md` tells integrators to
    /// map to HTTP status codes.
    Typed(ProofError),
    /// The invocation was aborted below `proof-registry`'s error surface: a
    /// host error, a failed conversion of a dependency's return value, or an
    /// error code that is not part of `ProofError`.
    ///
    /// This is what a caller observes when a dependency is missing, is
    /// version-incompatible, refuses the nested call, or when authorization
    /// fails. It carries no proof-registry semantics, which is the point: a
    /// boundary failure must never be mistaken for a registration verdict.
    Aborted,
}

impl Rejection {
    /// Classifies the result of a generated `try_*` client method.
    ///
    /// `try_*` returns `Result<Result<T, ConversionError>, Result<E, InvokeError>>`:
    /// the outer `Ok` is a completed invocation, `Err(Ok(_))` is the contract's
    /// own typed error, and `Err(Err(_))` is everything else.
    fn of<T, C>(outcome: Result<Result<T, C>, Result<ProofError, InvokeError>>) -> Self {
        match outcome {
            Ok(_) => Rejection::Accepted,
            Err(Ok(error)) => Rejection::Typed(error),
            Err(Err(_)) => Rejection::Aborted,
        }
    }
}

/// Runs one `try_*` attempt and classifies how it ended.
///
/// `try_*` turns a rejected invocation into `Err`. A failure originating
/// *below* the invoked contract — a trap inside a dependency — can instead
/// unwind as a Rust panic in this environment; `tests/events` already depends
/// on catching that. Either route means the same thing here: the invocation
/// aborted without producing a `ProofError`, which is [`Rejection::Aborted`].
/// Routing every attempt through one helper keeps that equivalence in a single
/// place instead of scattering `catch_unwind` through the scenarios.
///
/// `AssertUnwindSafe` is required and correct: `Env` carries interior
/// mutability, but observing its state after the failure is the entire point,
/// and it is discarded when the test ends.
pub fn outcome_of<T, C>(
    call: impl FnOnce() -> Result<Result<T, C>, Result<ProofError, InvokeError>>,
) -> Rejection {
    let previous = std::panic::take_hook();
    std::panic::set_hook(std::boxed::Box::new(|_| {}));
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call));
    std::panic::set_hook(previous);

    match caught {
        Ok(outcome) => Rejection::of(outcome),
        Err(_) => Rejection::Aborted,
    }
}

/// The observable state that a rejected registration must not move.
///
/// Deliberately wider than the proof record. An invocation that rolled back its
/// own write but left a dependency's counter advanced would still have broken
/// atomicity, and the proof record alone would not show it.
#[derive(Debug, PartialEq)]
pub struct Footprint {
    /// The proof record under test, or `None` when no entry exists.
    pub proof: Option<ProofRecord>,
    /// TTL of the proof entry; `None` when the entry does not exist.
    pub proof_ttl: Option<u32>,
    /// Instance TTL of `proof-registry`.
    pub proofs_instance_ttl: u32,
    /// Instance TTL of `protocol-config`.
    pub config_instance_ttl: u32,
    /// Instance TTL of `issuer-registry`.
    pub issuers_instance_ttl: u32,
    /// `protocol-config`'s monotonic mutation counter, `DataKey::ConfigVersion`.
    /// The only counter in the workspace, and the one a partial write would
    /// most plausibly advance.
    pub config_version: u32,
    /// `protocol-config`'s pause flag.
    pub paused: bool,
    /// Approval flag for [`APPROVED_SCHEMA`], read raw.
    pub schema_approved: bool,
    /// TTL of the `SchemaVersion(APPROVED_SCHEMA)` entry, read raw.
    ///
    /// `is_schema_version_approved` extends this entry as a side effect, so a
    /// registration that got past boundary 2 and then failed must have had that
    /// extension rolled back. See
    /// [`crate::boundaries::a_failed_registration_does_not_extend_the_schema_version_ttl`].
    pub schema_ttl: Option<u32>,
    /// The issuer record backing the registration.
    pub issuer: IssuerRecord,
    /// `proof-registry`'s administrator.
    pub proofs_admin: Address,
    /// The `protocol-config` address `proof-registry` holds.
    pub proofs_config_ref: Address,
    /// The `issuer-registry` address `proof-registry` holds.
    pub proofs_issuers_ref: Address,
}

/// A wired deployment: `protocol-config`, `issuer-registry`, one active issuer,
/// one approved schema version, and a `proof-registry` pointed at a chosen pair
/// of dependency addresses.
pub struct Deployment<'a> {
    pub env: Env,
    /// The real `protocol-config`. Always deployed, even when `proof-registry`
    /// has been pointed somewhere else.
    pub config: ProtocolConfigContractClient<'a>,
    /// The real `issuer-registry`, on the same terms.
    pub issuers: IssuerRegistryContractClient<'a>,
    pub proofs: ProofRegistryContractClient<'a>,
    pub admin: Address,
    /// The active issuer authorised to register proofs.
    pub issuer: Address,
    /// Id hash of [`Deployment::issuer`] in `issuer-registry`.
    pub issuer_id: BytesN<32>,
}

impl Deployment<'_> {
    /// A deployment wired to the real dependencies.
    pub fn new() -> Self {
        Self::with_dependency_addresses(|_env, config, issuers| (config, issuers))
    }

    /// The same fixture, but `proof-registry` is initialised against whichever
    /// pair `choose` returns.
    ///
    /// `choose` receives the environment, so a scenario can register a
    /// substitute dependency from [`crate::mocks`] and hand back its address.
    /// The real contracts stay deployed and reachable through
    /// [`Deployment::config`] and [`Deployment::issuers`], which is what lets a
    /// test show that the *referenced* contract — not merely some contract of
    /// the right shape — is the one that gates registration.
    pub fn with_dependency_addresses(
        choose: impl FnOnce(&Env, Address, Address) -> (Address, Address),
    ) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(START_TIMESTAMP);

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

        let (config_ref, issuers_ref) = choose(&env, config_id, issuers_id);

        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
        proofs.initialize(&admin, &issuers_ref, &config_ref);

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

    /// A valid expiry for a registration made now.
    pub fn expiry(&self) -> u64 {
        self.env.ledger().timestamp() + PROOF_LIFETIME
    }

    /// Captures the complete footprint the failure scenarios assert on.
    ///
    /// Every entry whose TTL appears in the result is read through
    /// `as_contract` so the capture itself extends nothing. The values that no
    /// read can disturb — the pause flag, the config version counter, the
    /// instance references — come from the public getters, which
    /// `docs/storage-model.md` records as non-extending.
    pub fn footprint(&self, proof_id: &BytesN<32>) -> Footprint {
        let key = proof_key(&self.env, proof_id);
        let (proof, proof_ttl) = self.env.as_contract(&self.proofs.address, || {
            let persistent = self.env.storage().persistent();
            if persistent.has(&key) {
                (
                    persistent.get::<_, ProofRecord>(&key),
                    Some(persistent.get_ttl(&key)),
                )
            } else {
                (None, None)
            }
        });

        let (schema_approved, schema_ttl) = self.env.as_contract(&self.config.address, || {
            let persistent = self.env.storage().persistent();
            let schema = schema_key(&self.env, APPROVED_SCHEMA);
            (
                persistent.get::<_, bool>(&schema).unwrap_or(false),
                persistent.has(&schema).then(|| persistent.get_ttl(&schema)),
            )
        });

        Footprint {
            proof,
            proof_ttl,
            proofs_instance_ttl: self.instance_ttl(&self.proofs.address),
            config_instance_ttl: self.instance_ttl(&self.config.address),
            issuers_instance_ttl: self.instance_ttl(&self.issuers.address),
            config_version: self.config.get_config_version(),
            paused: self.config.is_paused(),
            schema_approved,
            schema_ttl,
            issuer: self.issuers.get_issuer(&self.issuer_id),
            proofs_admin: self.proofs.get_admin(),
            proofs_config_ref: self.proofs.get_protocol_config(),
            proofs_issuers_ref: self.proofs.get_issuer_registry(),
        }
    }

    fn instance_ttl(&self, contract: &Address) -> u32 {
        self.env
            .as_contract(contract, || self.env.storage().instance().get_ttl())
    }

    /// Attempts a registration expected to succeed, and returns its proof id.
    pub fn register(&self, discriminator: u8) -> BytesN<32> {
        let proof_id = hash(&self.env, discriminator);
        self.proofs.register_proof(
            &proof_id,
            &commitment(&self.env, discriminator),
            &self.issuer,
            &APPROVED_SCHEMA,
            &self.expiry(),
        );
        proof_id
    }

    /// Attempts to register `proof_id` with otherwise valid arguments, and
    /// asserts the attempt was rejected, published nothing, and left the
    /// complete footprint unchanged.
    ///
    /// Returns the rejection so the caller can assert *which* error surfaced.
    pub fn assert_rejected_and_atomic(&self, proof_id: &BytesN<32>) -> Rejection {
        self.assert_rejected_and_atomic_with(proof_id, &self.issuer, APPROVED_SCHEMA, self.expiry())
    }

    /// As [`Deployment::assert_rejected_and_atomic`], with explicit arguments
    /// for the scenarios that reject on the arguments themselves.
    pub fn assert_rejected_and_atomic_with(
        &self,
        proof_id: &BytesN<32>,
        issuer: &Address,
        schema_version: u32,
        expires_at: u64,
    ) -> Rejection {
        let before = self.footprint(proof_id);

        let rejection = outcome_of(|| {
            self.proofs.try_register_proof(
                proof_id,
                &commitment(&self.env, 0xC0),
                issuer,
                &schema_version,
                &expires_at,
            )
        });

        // Events first. The environment reports the events of the *most recent*
        // invocation, not an accumulated log, so the getters inside
        // `footprint` would replace them.
        let events = self.env.events().all().events().len();
        let after = self.footprint(proof_id);

        assert_ne!(
            rejection,
            Rejection::Accepted,
            "the registration was expected to be rejected but succeeded"
        );
        assert_eq!(
            events, 0,
            "a rejected registration published {events} event(s); \
             a failed call must be indistinguishable from one never attempted"
        );
        assert_unchanged(&before, &after);
        rejection
    }
}

/// Asserts a rejected registration moved nothing, field by field.
///
/// Compared individually rather than with one `assert_eq!` on the whole
/// footprint so that a failure names the invariant that broke.
pub fn assert_unchanged(before: &Footprint, after: &Footprint) {
    assert_eq!(
        before.proof, after.proof,
        "the proof record changed during a rejected registration"
    );
    assert_eq!(
        before.proof_ttl, after.proof_ttl,
        "the proof entry TTL changed during a rejected registration"
    );
    assert_eq!(
        before.proofs_instance_ttl, after.proofs_instance_ttl,
        "the proof-registry instance TTL changed during a rejected registration"
    );
    assert_eq!(
        before.config_instance_ttl, after.config_instance_ttl,
        "the protocol-config instance TTL changed during a rejected registration"
    );
    assert_eq!(
        before.issuers_instance_ttl, after.issuers_instance_ttl,
        "the issuer-registry instance TTL changed during a rejected registration"
    );
    assert_eq!(
        before.config_version, after.config_version,
        "the protocol-config version counter advanced during a rejected registration"
    );
    assert_eq!(
        before.paused, after.paused,
        "the pause flag changed during a rejected registration"
    );
    assert_eq!(
        before.schema_approved, after.schema_approved,
        "a schema approval flag changed during a rejected registration"
    );
    assert_eq!(
        before.schema_ttl, after.schema_ttl,
        "the schema-version entry TTL changed during a rejected registration; \
         the extension performed by is_schema_version_approved was not rolled back"
    );
    assert_eq!(
        before.issuer, after.issuer,
        "the issuer record changed during a rejected registration"
    );
    assert_eq!(
        before.proofs_admin, after.proofs_admin,
        "the proof-registry administrator changed during a rejected registration"
    );
    assert_eq!(
        before.proofs_config_ref, after.proofs_config_ref,
        "the protocol-config reference changed during a rejected registration"
    );
    assert_eq!(
        before.proofs_issuers_ref, after.proofs_issuers_ref,
        "the issuer-registry reference changed during a rejected registration"
    );
}

/// Rebuilds `proof-registry`'s `DataKey::Proof(..)` storage key.
///
/// The variant is private to the contract crate, but the key it produces is
/// public ledger data and is documented as such in `docs/storage-model.md`.
/// `#[contracttype]` encodes an enum variant carrying data as
/// `[Symbol(variant), value]`, so the key can be rebuilt from outside.
///
/// [`crate::boundaries::the_reconstructed_proof_key_addresses_the_stored_record`]
/// fails loudly if that encoding ever changes, rather than letting every TTL
/// assertion in this crate quietly become vacuous.
pub fn proof_key(env: &Env, proof_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (Symbol::new(env, "Proof"), proof_id.clone())
}

/// Rebuilds `protocol-config`'s `DataKey::SchemaVersion(..)` storage key, on
/// the same terms as [`proof_key`].
pub fn schema_key(env: &Env, version: u32) -> (Symbol, u32) {
    (Symbol::new(env, "SchemaVersion"), version)
}

/// Deterministic 32-byte value derived from one discriminator byte.
pub fn hash(env: &Env, discriminator: u8) -> BytesN<32> {
    BytesN::from_array(env, &[discriminator; 32])
}

/// Commitment hash paired with a proof id, in a disjoint range so a scenario
/// mixing the two cannot collide.
pub fn commitment(env: &Env, discriminator: u8) -> BytesN<32> {
    hash(env, discriminator ^ 0xFF)
}
