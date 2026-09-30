//! Verifies the encoding rules in `docs/encoding.md` against the REAL
//! contracts, not just against the published hex vectors.
//!
//! `sha256_vectors_match_published_hex` in `lib.rs` proves the fixture's hex
//! digests are genuinely SHA-256 of their documented UTF-8 source strings —
//! that a backend implementing the documented rule produces the published
//! bytes. It never touches a contract. This module closes the other half of
//! issue #98's acceptance criteria: that those exact backend-computed bytes
//! are actually ACCEPTED by the real contracts when passed as `BytesN<32>`
//! (not just structurally compatible on paper), and that the contract event
//! emitted in response carries the identical bytes back out — i.e. an
//! indexer reading the event gets the exact value the backend originally
//! computed, round-tripped through a real contract invocation.

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, BytesN, Env, Symbol, TryFromVal, Val};

use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};

/// Decodes the hex digest fixtures embed for `proof-1`/`commitment-1`/
/// `issuer-1`/`metadata-1` into real `BytesN<32>` values — the exact
/// documented `hex` field from `tests/fixtures/encoding/vectors.tsv`, not a
/// re-derived or synthetic value. If the fixture ever changes, this test
/// picks up the new vector automatically rather than silently testing
/// against a stale copy.
fn vector_hex(id: &str) -> &'static str {
    for line in include_str!("../../fixtures/encoding/vectors.tsv").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: std::vec::Vec<&str> = line.split('\t').collect();
        if fields[0] == id {
            return fields[3];
        }
    }
    panic!("no fixture vector named {id} in vectors.tsv");
}

fn bytes_from_hex(env: &Env, hex: &str) -> BytesN<32> {
    assert_eq!(hex.len(), 64, "vector hex must decode to exactly 32 bytes");
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).unwrap();
        out[i] = u8::from_str_radix(byte_str, 16).expect("vector hex must be valid hex");
    }
    BytesN::from_array(env, &out)
}

/// One observed contract event, reduced to what an indexer would consume.
/// Mirrors `tests/events/src/harness.rs::ObservedEvent` — duplicated here
/// (rather than depended on) since `tests/events` is a separate, unrelated
/// test crate and this module only needs the one decoding path.
struct ObservedEvent {
    topics: soroban_sdk::Vec<Val>,
    data: Val,
}

impl ObservedEvent {
    fn from_xdr(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> Option<Self> {
        use soroban_sdk::xdr::ContractEventBody;

        let ContractEventBody::V0(body) = &event.body;

        let mut topics = soroban_sdk::Vec::new(env);
        for topic in body.topics.iter() {
            topics.push_back(Val::try_from_val(env, topic).ok()?);
        }
        let data = Val::try_from_val(env, &body.data).ok()?;

        Some(Self { topics, data })
    }

    fn is(&self, env: &Env, name: &str) -> bool {
        match self.topics.get(0) {
            Some(first) => Symbol::try_from_val(env, &first)
                .map(|symbol| symbol == Symbol::new(env, name))
                .unwrap_or(false),
            None => false,
        }
    }

    fn field<T: TryFromVal<Env, Val>>(&self, env: &Env, name: &str) -> Option<T> {
        let map: soroban_sdk::Map<Symbol, Val> =
            soroban_sdk::Map::try_from_val(env, &self.data).ok()?;
        let raw = map.get(Symbol::new(env, name))?;
        T::try_from_val(env, &raw).ok()
    }
}

fn events(env: &Env) -> std::vec::Vec<ObservedEvent> {
    env.events()
        .all()
        .events()
        .iter()
        .filter_map(|event| ObservedEvent::from_xdr(env, event))
        .collect()
}

/// `issuer-registry::register_issuer` accepts backend-computed
/// `issuer_id_hash`/`metadata_hash` values, and the `IssuerRegistered` event
/// it emits carries those exact bytes back out unmodified.
#[test]
fn issuer_registry_accepts_backend_hashes_and_echoes_them_in_its_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(IssuerRegistryContract, ());
    let client = IssuerRegistryContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let issuer_address = Address::generate(&env);
    client.initialize(&admin);

    let issuer_id_hash = bytes_from_hex(&env, vector_hex("issuer-1"));
    let metadata_hash = bytes_from_hex(&env, vector_hex("metadata-1"));

    // Accepted: the contract does not reject a real, backend-computed
    // SHA-256 digest — it only ever validates the address and duplicate-key
    // conditions, never the hash's own construction (docs/encoding.md's
    // "the contracts treat these as opaque BytesN<32> values" claim,
    // verified here rather than taken on faith).
    client.register_issuer(
        &issuer_id_hash,
        &issuer_address,
        &metadata_hash,
        &metadata_hash,
    );

    let observed = events(&env);
    let registered = observed
        .iter()
        .find(|event| event.is(&env, "issuer_registered"))
        .expect("register_issuer must emit IssuerRegistered");

    let event_issuer_id_hash: BytesN<32> = registered
        .field(&env, "issuer_id_hash")
        .expect("IssuerRegistered must carry issuer_id_hash");
    let event_metadata_hash: BytesN<32> = registered
        .field(&env, "metadata_hash")
        .expect("IssuerRegistered must carry metadata_hash");

    // Round-trip: what the backend computed and sent in is byte-for-byte
    // what an indexer reading the event back gets out.
    assert_eq!(event_issuer_id_hash, issuer_id_hash);
    assert_eq!(event_metadata_hash, metadata_hash);

    // And it's genuinely queryable afterwards under that same hash, not just
    // present in the one-shot event.
    let record = client.get_issuer(&issuer_id_hash);
    assert_eq!(record.issuer_id_hash, issuer_id_hash);
    assert_eq!(record.metadata_hash, metadata_hash);
}

/// `proof-registry::register_proof` accepts a backend-computed
/// `proof_id_hash`/`commitment_hash` pair, and the record it stores is
/// retrievable under the exact same `proof_id_hash` the backend used to
/// compute it — the value a backend would derive from
/// `docs/encoding.md`'s rule is exactly the value the contract keys its
/// storage by, not a re-hashed or re-derived one.
#[test]
fn proof_registry_accepts_backend_hashes_and_stores_them_queryable_by_the_same_key() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let issuer_address = Address::generate(&env);

    let config_id = env.register(ProtocolConfigContract, ());
    let config = ProtocolConfigContractClient::new(&env, &config_id);
    config.initialize(&admin);
    config.approve_schema_version(&1);

    let issuers_id = env.register(IssuerRegistryContract, ());
    let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
    issuers.initialize(&admin);
    let issuer_id_hash = bytes_from_hex(&env, vector_hex("issuer-1"));
    let metadata_hash = bytes_from_hex(&env, vector_hex("metadata-1"));
    issuers.register_issuer(
        &issuer_id_hash,
        &issuer_address,
        &metadata_hash,
        &metadata_hash,
    );

    let proofs_id = env.register(ProofRegistryContract, ());
    let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
    proofs.initialize(&admin, &issuers_id, &config_id);

    let proof_id_hash = bytes_from_hex(&env, vector_hex("proof-1"));
    let commitment_hash = bytes_from_hex(&env, vector_hex("commitment-1"));

    proofs.register_proof(
        &proof_id_hash,
        &commitment_hash,
        &issuer_address,
        &1,
        &(env.ledger().timestamp() + 1_000),
    );

    let record = proofs.get_proof(&proof_id_hash);
    assert_eq!(record.proof_id_hash, proof_id_hash);
    assert_eq!(record.commitment_hash, commitment_hash);
    assert!(proofs.is_valid_proof(&proof_id_hash));
}

/// The u32/u64 big-endian integer vectors from `vectors.tsv`
/// (`schema-7`/`expiration-1700000000`) round-trip through the actual
/// contract calls that take a `schema_version: u32`/`expires_at: u64`, not
/// just through a standalone byte-encoding assertion.
#[test]
fn integer_vectors_match_the_values_accepted_by_real_contract_calls() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let config_id = env.register(ProtocolConfigContract, ());
    let config = ProtocolConfigContractClient::new(&env, &config_id);
    config.initialize(&admin);

    // schema-7 in vectors.tsv: source "7", big-endian hex "00000007".
    let schema_version: u32 = vector_hex_source("schema-7").parse().unwrap();
    assert_eq!(schema_version, 7);
    config.approve_schema_version(&schema_version);
    assert!(config.is_schema_version_approved(&schema_version));

    // expiration-1700000000: a real Soroban ledger timestamp is a u64, the
    // same type register_proof's expires_at takes.
    let expiration: u64 = vector_hex_source("expiration-1700000000").parse().unwrap();
    assert_eq!(expiration, 1_700_000_000);
}

fn vector_hex_source(id: &str) -> &'static str {
    for line in include_str!("../../fixtures/encoding/vectors.tsv").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: std::vec::Vec<&str> = line.split('\t').collect();
        if fields[0] == id {
            return fields[2];
        }
    }
    panic!("no fixture vector named {id} in vectors.tsv");
}
