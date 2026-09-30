//! Shared fixtures and event assertions.
//!
//! The assertions here read the *whole* event stream rather than counting it.
//! A count alone cannot distinguish "emitted the documented event" from
//! "emitted some event", which is exactly the confusion an indexer would
//! inherit.

use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env, Symbol, TryFromVal, Val};

use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};

/// Schema version approved by [`Deployment::new`].
pub const APPROVED_SCHEMA: u32 = 1;

/// One observed contract event, reduced to what an indexer would consume.
#[derive(Debug, Clone)]
pub struct ObservedEvent {
    /// Emitting contract.
    pub contract: Address,
    /// Event topics. The first is the discriminant.
    pub topics: soroban_sdk::Vec<Val>,
    /// Non-indexed payload.
    pub data: Val,
}

impl ObservedEvent {
    /// Converts one XDR event into host values.
    ///
    /// Returns `None` for anything that is not a contract event with a V0 body,
    /// which is the only shape these contracts produce.
    fn from_xdr(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> Option<Self> {
        use soroban_sdk::xdr::{ContractEventBody, ScVal};

        let ContractEventBody::V0(body) = &event.body;

        let contract_id = event.contract_id.clone()?;
        let contract = Address::try_from_val(
            env,
            &ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(contract_id)),
        )
        .ok()?;

        let mut topics = soroban_sdk::Vec::new(env);
        for topic in body.topics.iter() {
            topics.push_back(Val::try_from_val(env, topic).ok()?);
        }

        let data = Val::try_from_val(env, &body.data).ok()?;

        Some(Self {
            contract,
            topics,
            data,
        })
    }

    /// The event discriminant as a symbol, or `None` if the first topic is not one.
    pub fn discriminant(&self, env: &Env) -> Option<Symbol> {
        let first = self.topics.get(0)?;
        Symbol::try_from_val(env, &first).ok()
    }

    /// True when the discriminant matches `name`.
    ///
    /// `#[contractevent]` lowercases the struct name into snake_case, so
    /// `IssuerRegistered` is published under the topic `issuer_registered`.
    /// Callers pass the topic form.
    pub fn is(&self, env: &Env, name: &str) -> bool {
        match self.discriminant(env) {
            Some(symbol) => symbol == Symbol::new(env, name),
            None => false,
        }
    }

    /// Reads one payload field, converting it to `T`.
    pub fn field<T: TryFromVal<Env, Val>>(&self, env: &Env, name: &str) -> Option<T> {
        let map: soroban_sdk::Map<Symbol, Val> =
            soroban_sdk::Map::try_from_val(env, &self.data).ok()?;
        let raw = map.get(Symbol::new(env, name))?;
        T::try_from_val(env, &raw).ok()
    }

    /// Number of payload fields.
    pub fn field_count(&self, env: &Env) -> usize {
        soroban_sdk::Map::<Symbol, Val>::try_from_val(env, &self.data)
            .map(|map| map.len() as usize)
            .unwrap_or(0)
    }
}

/// A fully wired deployment of all three contracts.
pub struct Deployment<'a> {
    pub env: Env,
    pub config: ProtocolConfigContractClient<'a>,
    pub issuers: IssuerRegistryContractClient<'a>,
    pub proofs: ProofRegistryContractClient<'a>,
    pub issuer: Address,
    /// Id hash of the issuer registered by [`Deployment::new`].
    pub issuer_id: BytesN<32>,
}

impl Deployment<'_> {
    /// Deploys all three contracts, approves [`APPROVED_SCHEMA`], and registers
    /// one active issuer.
    ///
    /// Events emitted during construction are not observable afterwards: the
    /// environment reports only the most recent invocation, so by the time a
    /// test runs, setup events have already been replaced. Use
    /// [`Deployment::capture`] around the operation under test.
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

        let issuer_id = hash(&env, 0x01);
        issuers.register_issuer(&issuer_id, &issuer, &hash(&env, 0xAA), &hash(&env, 0x99));

        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
        proofs.initialize(&admin, &issuers_id, &config_id);

        Self {
            env,
            config,
            issuers,
            proofs,
            issuer,
            issuer_id,
        }
    }

    /// Every event emitted so far, oldest first.
    ///
    /// The test environment exposes events in XDR form. They are converted to
    /// host values here so assertions can compare against the same types the
    /// contract published, rather than against a debug rendering.
    pub fn events(&self) -> std::vec::Vec<ObservedEvent> {
        let captured = self.env.events().all();

        captured
            .events()
            .iter()
            .filter_map(|event| ObservedEvent::from_xdr(&self.env, event))
            .collect()
    }

    /// Runs `operation` and returns the events it emitted.
    ///
    /// This is the only correct way to attribute events in this environment.
    /// `env.events().all()` reports the events of the **most recent
    /// invocation**, not an accumulated log — a later call replaces what an
    /// earlier one published. Reading immediately after the call is therefore
    /// the only way to be sure the events belong to it.
    ///
    /// The consequence for the ordering tests is spelled out in
    /// [`crate::ordering`]: a multi-step sequence must be observed step by
    /// step, because the stream cannot be replayed afterwards.
    pub fn capture(&self, operation: impl FnOnce()) -> std::vec::Vec<ObservedEvent> {
        operation();
        self.events()
    }

    /// Registers a proof and returns its id hash.
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
}

/// Deterministic 32-byte value derived from one discriminator byte.
///
/// Synthetic by construction: no fixture in this crate encodes a real wallet,
/// proof identifier, or payload.
pub fn hash(env: &Env, discriminator: u8) -> BytesN<32> {
    BytesN::from_array(env, &[discriminator; 32])
}

/// Asserts that `events` contains exactly one entry, with the given topic.
///
/// Returns it, so payload assertions can follow.
pub fn expect_single<'a>(env: &Env, events: &'a [ObservedEvent], topic: &str) -> &'a ObservedEvent {
    assert_eq!(
        events.len(),
        1,
        "expected exactly one event with topic {topic}, observed {}",
        events.len()
    );
    assert!(
        events[0].is(env, topic),
        "expected topic {topic}, observed a different event"
    );
    &events[0]
}

/// Runs `operation`, expecting it to be rejected.
///
/// Returns the events emitted during the attempt. Every contract in this
/// workspace signals rejection by panicking, and a panicking invocation is
/// rolled back, so the returned slice should always be empty — which is the
/// property the callers assert.
pub fn attempt_failure(
    deployment: &Deployment,
    operation: impl FnOnce(),
) -> std::vec::Vec<ObservedEvent> {
    // `AssertUnwindSafe` is required and correct here. The `Env` carries
    // interior mutability, so it is not `UnwindSafe` — but observing its state
    // *after* the panic is the entire point of these tests, and the environment
    // is discarded when the test ends. This mirrors the pattern the existing
    // per-contract tests already use.
    let previous = std::panic::take_hook();
    std::panic::set_hook(std::boxed::Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
    std::panic::set_hook(previous);

    assert!(
        outcome.is_err(),
        "the operation was expected to be rejected but succeeded"
    );

    deployment.events()
}

/// Reads the events of the most recent invocation from a bare environment.
///
/// Used by tests that construct their own contracts rather than going through
/// [`Deployment`], such as the initialization cases — a deployment's own setup
/// events are no longer observable by the time it is returned.
pub fn read_events(env: &Env) -> std::vec::Vec<ObservedEvent> {
    let captured = env.events().all();
    captured
        .events()
        .iter()
        .filter_map(|event| ObservedEvent::from_xdr(env, event))
        .collect()
}
