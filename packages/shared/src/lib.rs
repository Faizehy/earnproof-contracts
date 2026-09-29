#![no_std]

use soroban_sdk::{contracterror, contracttype, Address, BytesN};

pub mod storage_namespaces;

pub use storage_namespaces::{StorageClass, StorageNamespace, STORAGE_NAMESPACES};
pub mod error_catalog;

pub use error_catalog::{Domain, ErrorSpec, Retry, Status, ERROR_CATALOG};

pub const TTL_THRESHOLD_LEDGERS: u32 = 50_000;

/// Target ledgers for extended TTL after triggering a preemptive extension.
pub const TTL_EXTEND_TO_LEDGERS: u32 = 500_000;

// A Stellar strkey address (G...) is always exactly 56 ASCII characters.
// soroban_sdk::String has no .chars() (unlike std::string::String, and
// unlike Symbol, this isn't even gated off-WASM only - it simply doesn't
// exist on any target in this SDK version) and doesn't implement
// PartialEq<&str>, only String == String - copy_into_slice() into a fixed
// buffer and comparing raw ASCII bytes is the actual supported way to
// inspect a soroban_sdk::String's contents on every target.
const STRKEY_ADDRESS_LEN: usize = 56;

fn address_bytes(address: &Address) -> [u8; STRKEY_ADDRESS_LEN] {
    let value = address.to_string();
    let mut buf = [0u8; STRKEY_ADDRESS_LEN];
    if value.len() as usize == STRKEY_ADDRESS_LEN {
        value.copy_into_slice(&mut buf);
    }
    buf
}

// The strkey encoding of an all-zero (32-byte) ed25519 public key: version
// byte 'G' + 32 zero payload bytes + a real CRC16/XMODEM checksum over
// those 33 bytes, base32-encoded. The checksum is NOT itself all zero bits
// (a correct checksum over an all-zero payload is not the all-zero
// checksum), so this string does not end in all 'A's — comparing the
// full string against this one known-correct value is the only way to
// recognize it; a pattern check like "G followed by all A's" would (and
// previously did) silently never match a real, correctly-checksummed
// all-zero-payload address at all.
const ZERO_PAYLOAD_STRKEY: &[u8; STRKEY_ADDRESS_LEN] =
    b"GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

pub fn is_zero_or_sentinel_address(address: &Address) -> bool {
    let bytes = address_bytes(address);
    &bytes == ZERO_PAYLOAD_STRKEY
}

pub fn is_valid_principal_address(address: &Address) -> bool {
    let value = address.to_string();
    if value.is_empty() || value.len() as usize != STRKEY_ADDRESS_LEN {
        return false;
    }
    let bytes = address_bytes(address);
    if is_zero_or_sentinel_address(address) {
        return false;
    }
    bytes
        .iter()
        .all(|&byte| matches!(byte, b'A'..=b'Z' | b'2'..=b'7'))
}

// ---------------------------------------------------------------------------
// Error Codes
//
// Error ranges are allocated to prevent collisions:
// - Common errors:       1-99
// - Protocol Config:     100-199
// - Issuer Registry:     200-299
// - Proof Registry:      300-399
//
// Each error code is stable and machine-readable. Backend integrations
// should map these codes to appropriate HTTP status codes and user messages.
// ---------------------------------------------------------------------------

/// Common errors shared across all contracts.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    // Initialization errors (1-19)
    AlreadyInitialized = 1,
    NotInitialized = 2,

    // Authorization errors (20-39)
    Unauthorized = 20,

    // State errors (40-59)
    AlreadyExists = 40,
    NotFound = 41,
    InvalidState = 42,

    // Input validation errors (60-79)
    InvalidInput = 60,
    InvalidAddress = 61,

    // Protocol state errors (80-99)
    ProtocolPaused = 80,

    // Batch query errors (62, in the input-validation range)
    /// A batch query supplied more items than the documented maximum.
    BatchTooLarge = 62,
}

/// Issuer-specific errors (200-299).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum IssuerError {
    IssuerAlreadyRegistered = 200,
    IssuerNotFound = 201,
    IssuerAddressAlreadyRegistered = 202,
    IssuerAddressNotFound = 203,
    IssuerRevoked = 204,
    IssuerInactive = 205,
    InvalidTransition = 206,
    InvalidAddress = 207,
    /// A batch query supplied more identifiers than [`MAX_ISSUER_STATUS_BATCH`].
    BatchTooLarge = 208,
}

/// Proof-specific errors (300-399).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ProofError {
    ProofAlreadyRegistered = 300,
    ProofNotFound = 301,
    ProofAlreadyRevoked = 302,
    ProofExpired = 303,
    InvalidSchemaVersion = 304,
    SchemaVersionNotApproved = 305,
    InvalidAddress = 306,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IssuerStatus {
    Active,
    Suspended,
    Revoked,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProofStatus {
    Active,
    Revoked,
}

/// Upper bound on the number of identifiers a single bounded batch issuer
/// status query may carry. The limit is enforced before any storage access so
/// an oversized request cannot force unbounded host work.
pub const MAX_ISSUER_STATUS_BATCH: u32 = 50;

/// Status of a single issuer as reported by a bounded batch status query.
///
/// Unlike [`IssuerStatus`], this carries an explicit `NotFound` so an unknown
/// identifier is unambiguous rather than being conflated with any live state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IssuerQueryStatus {
    Active,
    Suspended,
    Revoked,
    NotFound,
}

/// One entry in a bounded batch issuer status response.
///
/// The identifier is echoed back next to its status so callers can correlate
/// results by value; combined with preserved input ordering this makes
/// duplicate identifiers in the request unambiguous in the response.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuerStatusResult {
    pub issuer_id_hash: BytesN<32>,
    pub status: IssuerQueryStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuerRecord {
    pub issuer_id_hash: BytesN<32>,
    pub issuer_address: Address,
    pub metadata_hash: BytesN<32>,
    pub status: IssuerStatus,
    pub created_at: u64,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofRecord {
    pub proof_id_hash: BytesN<32>,
    pub commitment_hash: BytesN<32>,
    pub issuer_address: Address,
    pub status: ProofStatus,
    pub schema_version: u32,
    pub expires_at: u64,
    pub created_at: u64,
    /// Ledger timestamp at which the proof was revoked. `0` while the proof is
    /// active.
    pub revoked_at: u64,
    /// Ledger sequence at which the proof was revoked. `0` while the proof is
    /// active, and `0` for a legacy record revoked before the sequence was
    /// recorded — the timestamp remains authoritative in that case.
    pub revoked_ledger: u32,
}

/// Structured validity answer for a single proof, delivering the effective
/// revocation timing alongside the validity verdict.
///
/// The `revoked` flag is the guard for the timing fields: when it is `false`
/// (any proof that is not revoked) `revoked_at` and `revoked_ledger` are `0` and
/// carry no meaning, so an active proof never exposes a fabricated revocation
/// time. When it is `true`, `revoked_at` and `revoked_ledger` are the timing
/// captured atomically at revocation; `revoked_ledger` is `0` for a legacy
/// record revoked before the sequence was recorded, where the timestamp remains
/// authoritative.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofValidity {
    pub status: ProofStatus,
    pub is_valid: bool,
    pub expires_at: u64,
    pub revoked: bool,
    pub revoked_at: u64,
    pub revoked_ledger: u32,
}

/// Upper bound on the number of versions a single bounded batch schema status
/// query may carry. The limit is enforced before any storage access so an
/// oversized request cannot force unbounded host work.
pub const MAX_SCHEMA_STATUS_BATCH: u32 = 50;

/// Lifecycle state of a schema version, as reported by a bounded batch status
/// query.
///
/// The three states are mutually exclusive and cover the whole lifecycle:
/// `Unknown` is a version that was never approved (no stored record), `Approved`
/// is one that is currently active, and `Deprecated` is one that was approved
/// and later withdrawn. Keeping `Deprecated` distinct from `Unknown` lets a
/// caller tell "never seen" apart from "withdrawn".
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaVersionState {
    Unknown,
    Approved,
    Deprecated,
}

/// One entry in a bounded batch schema status response.
///
/// The version is echoed back next to its state so callers can correlate
/// results by value; combined with preserved input ordering this makes
/// duplicate versions in the request unambiguous in the response.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaStatusResult {
    pub version: u32,
    pub state: SchemaVersionState,
}

/// Maximum length of a schema lineage chain the contract will walk when
/// validating a predecessor or answering a lineage query. It bounds the storage
/// reads a single call can trigger and, together with the self-predecessor and
/// cycle checks, keeps lineage traversal terminating.
pub const MAX_SCHEMA_LINEAGE_DEPTH: u32 = 32;

// ── Shared Test Utilities ──────────────────────────────────────────────────────
// These utilities provide common patterns for initialization adversarial testing
// across all contracts, ensuring consistent test coverage for re-initialization
// guards, invalid dependencies, and state/event immutability on failure.

#[cfg(test)]
pub mod test_utils {
    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    /// Represents the expected state after successful initialization.
    /// Used to verify that first initialization produces exactly the documented
    /// state with no partial writes.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct InitializedState {
        /// The admin address that was set during initialization.
        pub admin: Address,
        /// True if the contract emitted an event during initialization.
        pub event_emitted: bool,
        /// Additional state keys that should be present after initialization.
        pub expected_keys: Vec<&'static str>,
    }

    /// Test result for re-initialization attempts.
    /// Captures whether the attempt failed and whether state remained unchanged.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ReinitAttemptResult {
        /// True if re-initialization attempt failed (panicked or errored).
        pub failed: bool,
        /// True if storage state is byte-for-byte identical before and after attempt.
        pub state_unchanged: bool,
        /// True if no new events were emitted during the failed attempt.
        pub no_new_events: bool,
    }

    /// Test result for invalid dependency/configuration initialization attempts.
    /// Captures whether the attempt failed and whether state remained atomic.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct InvalidDependencyResult {
        /// True if initialization attempt failed.
        pub failed: bool,
        /// True if storage state is unchanged after the failed attempt.
        pub atomic_failure: bool,
        /// True if no events were emitted during the failed attempt.
        pub no_events: bool,
    }

    /// Documents the initialization contract's behavior for test purposes.
    /// This structure is filled out for each contract being tested and serves
    /// as the specification against which adversarial tests validate behavior.
    #[derive(Clone, Debug)]
    pub struct ContractInitSpec {
        /// Name of the contract being tested.
        pub contract_name: &'static str,
        /// True if this contract has a re-initialization guard.
        pub has_reinit_guard: bool,
        /// True if this contract emits an event during initialization.
        pub emits_init_event: bool,
        /// True if this contract takes dependency addresses as initialization parameters.
        pub takes_dependencies: bool,
        /// List of dependency contract names this contract requires (e.g., ["issuer-registry", "protocol-config"]).
        pub dependency_names: Vec<&'static str>,
    }

    impl ContractInitSpec {
        /// Helper to create a spec for a standalone contract with a re-initialization guard.
        pub fn standalone_with_guard(name: &'static str, emits_event: bool) -> Self {
            ContractInitSpec {
                contract_name: name,
                has_reinit_guard: true,
                emits_init_event: emits_event,
                takes_dependencies: false,
                dependency_names: vec![],
            }
        }

        /// Helper to create a spec for a contract with dependencies and a re-initialization guard.
        pub fn with_dependencies_and_guard(
            name: &'static str,
            deps: Vec<&'static str>,
            emits_event: bool,
        ) -> Self {
            ContractInitSpec {
                contract_name: name,
                has_reinit_guard: true,
                emits_init_event: emits_event,
                takes_dependencies: true,
                dependency_names: deps,
            }
        }
    }
}
