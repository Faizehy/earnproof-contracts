#![no_std]

use soroban_sdk::{contracterror, contracttype, xdr::ToXdr, Address, BytesN, Env, Symbol};

pub mod storage_namespaces;

pub use storage_namespaces::{StorageClass, StorageNamespace, STORAGE_NAMESPACES};
pub mod error_catalog;

pub use error_catalog::{Domain, ErrorSpec, Retry, Status, ERROR_CATALOG};

// Export upgrade approval types for use across all contracts
pub use soroban_sdk::String as SorobanString;

pub const TTL_THRESHOLD_LEDGERS: u32 = 50_000;

/// Target ledgers for extended TTL after triggering a preemptive extension.
pub const TTL_EXTEND_TO_LEDGERS: u32 = 500_000;

/// Sentinel written into ledger-sequence metadata fields for records that were
/// created before those fields existed (legacy records). A live ledger
/// sequence is always >= 1, so `0` is an unambiguous "unknown / not recorded"
/// marker that consumers can detect and treat as legacy.
pub const LEDGER_SEQUENCE_UNSET: u32 = 0;

/// Sentinel written into ledger-timestamp metadata fields for legacy records.
/// A live ledger timestamp is always > 0, so `0` unambiguously marks
/// "unknown / not recorded".
pub const LEDGER_TIMESTAMP_UNSET: u64 = 0;

/// Initial metadata revision assigned to an issuer at registration. Each
/// accepted metadata update increments the revision by one.
pub const METADATA_REVISION_INITIAL: u32 = 1;

/// Minimum ledgers between approval and execution (timelock).
/// Prevents immediate execution of just-approved upgrades.
/// ~1 day at 5s/ledger = 17,280 ledgers
pub const UPGRADE_TIMELOCK_LEDGERS: u32 = 17_280;

/// Maximum ledgers an approval remains valid after creation.
/// Stale approvals expire and must be re-approved.
/// ~30 days at 5s/ledger = 518_400 ledgers
pub const UPGRADE_APPROVAL_EXPIRY_LEDGERS: u32 = 518_400;
/// Storage layout version for the migration checkpoint record.
pub const MIGRATION_STATUS_VERSION: u32 = 1;

/// Maximum number of records a single migration invocation may commit.
pub const MAX_MIGRATION_BATCH: u32 = 100;

/// Maximum number of proofs a single batch registration or batch revocation
/// call may contain. Bounded not just by CPU/memory but by Soroban's
/// per-invocation ledger footprint limit (100 entries in this environment):
/// each proof touches a persistent data entry and its TTL entry, and a
/// batch revocation touching state written by prior calls was measured to
/// exceed that footprint limit at 25. 20 leaves comfortable headroom on
/// both the registration and revocation paths.
pub const MAX_PROOF_BATCH_SIZE: u32 = 20;

/// Resumable progress marker shared by every contract upgrade path.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStatus {
    pub status_version: u32,
    pub target_contract_version: u32,
    pub cursor: u32,
    pub total_items: u32,
    pub complete: bool,
}

/// Canonical configuration digest payload version.
pub const CONFIG_DIGEST_VERSION: u32 = 1;

/// Version tag mixed into every computed genesis identifier so a future
/// change to the derivation scheme is distinguishable from a collision.
pub const GENESIS_ID_VERSION: u32 = 1;

/// Fallback maximum auxiliary payload size (in bytes) applied to a schema
/// version that has no explicit override configured in protocol-config.
pub const DEFAULT_SCHEMA_PAYLOAD_LIMIT: u32 = 4096;

/// Default deterministic ledger window used when a schema has no governed rate
/// limit. A zero maximum means registrations are paused for that schema.
pub const DEFAULT_SCHEMA_RATE_WINDOW_LEDGERS: u32 = 1_000;
pub const DEFAULT_SCHEMA_RATE_LIMIT: u32 = u32::MAX;

/// Governed, fixed-size issuance policy for one schema version.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaRateLimit {
    pub max_registrations: u32,
    pub window_ledgers: u32,
}

/// Observable usage for the current deterministic schema window.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaRateLimitUsage {
    pub window_start_ledger: u32,
    pub reset_ledger: u32,
    pub registrations: u32,
    pub remaining: u32,
}

/// Computes a deterministic, domain-separated genesis identifier for a
/// contract instance.
///
/// The identifier is derived from a version tag, a role-specific domain
/// symbol (e.g. `"earnproof_proof_registry"`), the network passphrase
/// digest, and this contract's own address. Because the domain differs per
/// contract role and the network id differs per network, two instances can
/// never share an identity by accident, even if deployed from the same WASM
/// to the same address space on different networks.
///
/// The result is immutable by construction: every input is fixed at the
/// moment `initialize` runs and never changes afterwards.
pub fn compute_genesis_id(env: &Env, domain: &str) -> BytesN<32> {
    let payload = (
        GENESIS_ID_VERSION,
        Symbol::new(env, domain),
        env.ledger().network_id(),
        env.current_contract_address(),
    )
        .to_xdr(env);
    env.crypto().sha256(&payload).to_bytes()
}

/// Immutable deployment identity, written once during `initialize` and
/// carried unchanged across upgrades and storage migrations.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenesisRecord {
    /// Domain-separated identifier computed by [`compute_genesis_id`].
    pub genesis_id: BytesN<32>,
    /// Ledger sequence at which `initialize` committed this record.
    pub initialized_at_ledger: u32,
}

/// Bounded record of an auxiliary proof payload accepted alongside a
/// registration. Only the length and a commitment hash are kept on-chain;
/// the raw payload itself is never stored, so resource use stays bounded
/// regardless of the configured schema limit.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofPayloadRecord {
    /// Length in bytes of the auxiliary payload supplied at registration.
    pub payload_len: u32,
    /// SHA-256 hash of the auxiliary payload.
    pub payload_hash: BytesN<32>,
}

pub fn protocol_config_digest(
    env: &Env,
    admin: &Address,
    paused: bool,
    config_version: u32,
    contract_version: u32,
) -> BytesN<32> {
    let payload = (
        CONFIG_DIGEST_VERSION,
        Symbol::new(env, "earnproof_protocol_config"),
        admin.clone(),
        paused,
        config_version,
        contract_version,
    )
        .to_xdr(env);
    env.crypto().sha256(&payload).to_bytes()
}

pub fn issuer_registry_digest(env: &Env, admin: &Address, contract_version: u32) -> BytesN<32> {
    let payload = (
        CONFIG_DIGEST_VERSION,
        Symbol::new(env, "earnproof_issuer_registry"),
        admin.clone(),
        contract_version,
    )
        .to_xdr(env);
    env.crypto().sha256(&payload).to_bytes()
}

pub fn proof_registry_digest(
    env: &Env,
    admin: &Address,
    issuer_registry: &Address,
    protocol_config: &Address,
    contract_version: u32,
) -> BytesN<32> {
    let payload = (
        CONFIG_DIGEST_VERSION,
        Symbol::new(env, "earnproof_proof_registry"),
        admin.clone(),
        issuer_registry.clone(),
        protocol_config.clone(),
        contract_version,
    )
        .to_xdr(env);
    env.crypto().sha256(&payload).to_bytes()
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TtlHealth {
    Missing,
    NearExpiry,
    Healthy,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtlStatus {
    pub health: TtlHealth,
    pub remaining_ledgers: u32,
    pub threshold_ledgers: u32,
}

pub fn ttl_status(current_ledger: u32, exists: bool, live_until: Option<u32>) -> TtlStatus {
    let remaining = live_until
        .filter(|_| exists)
        .map(|ledger| ledger.saturating_sub(current_ledger))
        .unwrap_or(0);
    let health = if !exists || live_until.is_none() || remaining == 0 {
        TtlHealth::Missing
    } else if remaining <= TTL_THRESHOLD_LEDGERS {
        TtlHealth::NearExpiry
    } else {
        TtlHealth::Healthy
    };
    TtlStatus {
        health,
        remaining_ledgers: remaining,
        threshold_ledgers: TTL_THRESHOLD_LEDGERS,
    }
}

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

// ---------------------------------------------------------------------------
// Dependency interface versioning
//
// Cross-contract dependencies expose a machine-readable interface version so
// that a consumer (e.g. proof-registry) can refuse to bind to a dependency
// whose interface it does not understand.
//
// The version follows a semver-style major/minor/patch tuple:
//   - A `major` bump is a breaking change: the consumer must match it exactly.
//   - `minor`/`patch` are backward compatible within the same `major`: a
//     dependency may advance them freely and remain acceptable, but it must be
//     at least the minimum the consumer requires.
//
// Compatibility rule (see `is_interface_compatible`):
//   actual.major == required.major
//     && (actual.minor, actual.patch) >= (required.minor, required.patch)
// ---------------------------------------------------------------------------

/// A machine-readable interface version exposed by a cross-contract dependency.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InterfaceVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl InterfaceVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        InterfaceVersion {
            major,
            minor,
            patch,
        }
    }
}

/// The interface version implemented by `issuer-registry`.
pub const ISSUER_REGISTRY_INTERFACE_VERSION: InterfaceVersion = InterfaceVersion::new(1, 0, 0);

/// The interface version implemented by `protocol-config`.
pub const PROTOCOL_CONFIG_INTERFACE_VERSION: InterfaceVersion = InterfaceVersion::new(1, 0, 0);

/// Returns true when `actual` is compatible with the `required` minimum.
///
/// The `major` component must match exactly (a breaking-change boundary); the
/// `minor`/`patch` components of `actual` must be greater than or equal to the
/// required minimum, compared lexicographically. Newer compatible dependencies
/// (higher minor/patch, same major) are therefore accepted.
pub fn is_interface_compatible(required: &InterfaceVersion, actual: &InterfaceVersion) -> bool {
    actual.major == required.major
        && (actual.minor, actual.patch) >= (required.minor, required.patch)
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
    // Batch query bounds (shared query rejection).
    BatchTooLarge = 62,
    /// A cross-contract dependency reported an interface version outside the
    /// range the consumer accepts.
    IncompatibleInterfaceVersion = 63,

    // Protocol state errors (80-99)
    ProtocolPaused = 80,

    // Upgrade timing errors (90-99)
    NoUpgradeApproval = 90,
    UpgradeTimelockNotElapsed = 91,
    UpgradeApprovalExpired = 92,
    WasmHashMismatch = 93,
    InvalidTimingConfig = 94,

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
    /// Registering or reactivating this issuer would exceed the governed
    /// maximum active-issuer capacity.
    IssuerCapacityExceeded = 211,
    /// The metadata commitment did not match the required fixed-size format.
    InvalidMetadataCommitment = 212,
    /// A batch query supplied more identifiers than [`MAX_ISSUER_STATUS_BATCH`].
    BatchTooLarge = 208,
    /// A requested capacity limit is below the current active-issuer usage and
    /// no explicit override was supplied.
    MaxBelowActiveUsage = 209,
    /// The suspended issuer's reactivation cooldown has not yet elapsed.
    ReactivationCooldownActive = 210,
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
    // Separated precondition errors (307-310)
    /// Contract is paused — proof registration is temporarily disabled.
    /// Recovery: monitor for unpause event before retrying.
    ContractPaused = 307,
    /// Issuer account is not active or not authorized to register proofs.
    /// Distinct from authorization failure — the issuer exists but is inactive.
    /// Recovery: contact platform to activate the issuer account.
    IssuerInactive = 308,
    /// The proof schema identifier is not supported or not registered.
    /// Distinct from malformed input — the schema reference is well-formed
    /// but unknown to this contract.
    /// Recovery: check supported schemas via get_supported_schemas().
    UnsupportedSchema = 309,
    /// Proof input data is malformed — fails format or size validation.
    /// Distinct from unsupported schema — the input itself is invalid.
    /// Recovery: validate input against the schema before resubmitting.
    MalformedInput = 310,
    /// A batch operation was given zero entries or more than
    /// `MAX_PROOF_BATCH_SIZE` entries.
    /// Recovery: split the batch into chunks of at most `MAX_PROOF_BATCH_SIZE`.
    InvalidBatchSize = 311,
    /// `register_proof_with_activation` was given an `activates_at` at or
    /// after `expires_at`, so the proof could never be valid.
    /// Recovery: choose an activation time strictly before the expiration.
    InvalidActivationTime = 312,
    /// `open_dispute` was called for a proof that already has an `Open`
    /// dispute. Recovery: withdraw, resolve, or reject the existing dispute
    /// before opening a new one — retrying the identical request will not
    /// help, since the dispute is cleared by a different call, not by this
    /// one succeeding on its own.
    DisputeAlreadyOpen = 313,
    /// `withdraw_dispute`, `resolve_dispute`, or `reject_dispute` referenced
    /// a proof with no dispute record.
    /// Recovery: open a dispute first, or confirm the proof id.
    DisputeNotFound = 314,
    /// A dispute transition was attempted on a dispute that is not `Open`
    /// (already withdrawn, resolved, or rejected).
    /// Recovery: read the dispute's current status; it is terminal.
    DisputeNotOpen = 315,
    /// A proof cannot supersede itself or create a supersession cycle.
    CyclicSupersession = 316,
    /// Supersession is restricted to proofs from the same issuer.
    CrossIssuerSupersession = 317,
    /// The specified predecessor proof was not found.
    PredecessorNotFound = 318,
    /// The predecessor already has the maximum number of successors.
    TooManySuccessors = 319,
    /// The proof type is not supported by the protocol.
    UnsupportedProofType = 320,
}

/// Fixed capacity of the protocol-config change-history ring. Once this many
/// entries have been recorded, the oldest entry is overwritten by the next
/// append — rollover is deterministic rather than unbounded growth.
pub const CONFIG_HISTORY_CAPACITY: u32 = 32;

/// Maximum number of entries a single `get_config_history` call may return,
/// regardless of the requested limit, so a query cannot be used to force an
/// unbounded read.
pub const MAX_CONFIG_HISTORY_PAGE: u32 = 20;

/// Category of a recorded protocol-config change. Distinct from the raw
/// parameter value: history entries carry only this tag, a commitment to the
/// changed value, and version/ledger metadata — never the sensitive value
/// itself.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ConfigChangeCategory {
    AdminRotation,
    PauseToggle,
    ScopedPause,
    SchemaApproval,
    SchemaDeprecation,
    SchemaPayloadLimit,
}

/// One bounded, on-chain summary of a governance change, as stored in the
/// change-history ring.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigChangeSummary {
    /// What kind of change this was.
    pub category: ConfigChangeCategory,
    /// Commitment to the changed value(s); never the raw value.
    pub proposal_commitment: BytesN<32>,
    /// Configuration version in effect immediately after this change.
    pub config_version: u32,
    /// Ledger sequence at which the change committed.
    pub ledger: u32,
    /// Ledger timestamp at which the change committed.
    pub timestamp: u64,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PauseScope {
    Global,
    Registration,
    Updates,
    Revocation,
    Upgrades,
    Disputes,
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

/// Stores temporal metadata for an upgrade approval.
///
/// # Timing invariants
/// - `created_at` ≤ `earliest_execution` ≤ `expires_at`
/// - execution is rejected before `earliest_execution`
/// - execution is rejected at or after `expires_at`
/// - re-approval resets ALL three fields (no stale reuse)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeApproval {
    /// WASM hash approved for upgrade
    pub wasm_hash: BytesN<32>,
    /// Ledger sequence when approval was created
    pub created_at: u32,
    /// Earliest ledger at which execution is permitted
    /// = created_at + UPGRADE_TIMELOCK_LEDGERS
    pub earliest_execution: u32,
    /// Ledger sequence after which approval is invalid
    /// = created_at + UPGRADE_APPROVAL_EXPIRY_LEDGERS
    pub expires_at: u32,
    /// Address that created this approval
    pub approved_by: Address,
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

/// Privacy-safe issuer signing-key commitment. Only a key digest and algorithm
/// identifier are persisted; raw public or private keys are never accepted.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SigningKeyCommitment {
    pub key_hash: BytesN<32>,
    pub algorithm: u32,
    pub activated_ledger: u32,
}

/// Versioned opaque commitments for issuer classification policy documents.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuerPolicyCommitments {
    pub encoding_version: u32,
    pub category_commitment: BytesN<32>,
    pub jurisdiction_commitment: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuerRecord {
    pub issuer_id_hash: BytesN<32>,
    pub issuer_address: Address,
    /// Hash commitment over the canonical issuer metadata document (content
    /// hash). See the `metadata-commitment` docs for the domain-separation
    /// and canonical-byte rules a backend must follow to reproduce it.
    pub metadata_hash: BytesN<32>,
    /// Hash commitment over the canonical metadata document URI (location
    /// hash), stored separately from `metadata_hash` so off-chain resolvers
    /// can distinguish a change of location from a change of content. A value
    /// of all-zero bytes is the documented "no URI commitment recorded"
    /// sentinel (used for records registered before a URI commitment was set).
    pub metadata_uri_hash: BytesN<32>,
    /// Monotonically increasing revision, starting at
    /// [`METADATA_REVISION_INITIAL`]. Each accepted metadata update increments
    /// it by one.
    pub metadata_revision: u32,
    pub provenance_commitment: BytesN<32>,
    pub status: IssuerStatus,
    pub created_at: u64,
    pub updated_at: u64,
    /// Ledger sequence at which the current `status` became effective.
    /// [`LEDGER_SEQUENCE_UNSET`] marks a legacy record predating this field.
    pub status_effective_ledger: u32,
    /// Ledger timestamp at which the current `status` became effective.
    /// [`LEDGER_TIMESTAMP_UNSET`] marks a legacy record predating this field.
    pub status_effective_timestamp: u64,
    pub reason_commitment: Option<BytesN<32>>,
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
    /// Ledger sequence at which the proof was revoked; zero while active.
    pub revoked_ledger: u32,
    pub predecessor_id_hash: Option<BytesN<32>>,
    pub proof_type: Option<BytesN<32>>,
    /// Ledger sequence at which this proof was created (registered).
    /// [`LEDGER_SEQUENCE_UNSET`] marks a legacy record predating this field.
    pub created_ledger: u32,
    /// Ledger timestamp at or after which this proof is considered active.
    /// `0` means the proof was registered without a delay and is active
    /// immediately (subject to `status` and `expires_at` as before). This
    /// field is fixed at registration and is never mutated afterward — there
    /// is no operation that moves it, earlier or later.
    pub activates_at: u64,
}

/// The full validity state of a proof, distinguishing every reason a proof
/// might not currently verify from the single boolean `is_valid_proof`
/// returns.
///
/// Two independent queries return this type, each populating a different
/// subset of variants:
///
/// - [`ProofRegistryContract::get_proof_validity`] performs only
///   locally-checkable comparisons against the stored record (status,
///   `activates_at`, `expires_at`) and reports one of `Active`, `Pending`,
///   `Revoked`, `Expired`, or `NotFound`.
/// - [`ProofRegistryContract::proof_validity`] additionally resolves the
///   issuer-registry and protocol-config dependencies to check whether the
///   issuer is still active and the schema version still approved,
///   evaluated in a documented, deterministic order so that when several
///   invalid conditions hold at once the earliest is the reported primary
///   reason: `Unknown` (no record), `Revoked`, `Expired`,
///   `IssuerInactive`, `SchemaDeprecated`, then `Valid`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProofValidity {
    /// `status == Active`, `activates_at` has been reached, and `expires_at`
    /// has not.
    Active,
    /// Registered and not revoked, but the ledger has not yet reached
    /// `activates_at`. Carries that timestamp (`Pending(activates_at)`) so a
    /// caller can know when to check again.
    Pending(u64),
    /// `status == Revoked`. Terminal: a revoked proof never becomes valid
    /// again, including one revoked while still pending.
    Revoked,
    /// Active and past its activation time, but at or after `expires_at`.
    Expired,
    /// No record exists for this proof id.
    NotFound,
    /// Returned by `proof_validity` instead of [`Self::NotFound`] when no
    /// record exists, or when the contract's dependency addresses cannot be
    /// resolved and validity cannot be asserted.
    Unknown,
    /// Returned by `proof_validity`: the record exists, is unexpired and
    /// unrevoked, but its issuing address is no longer active.
    IssuerInactive,
    /// Returned by `proof_validity`: the record exists, is unexpired and
    /// unrevoked, and its issuer is active, but its schema version is no
    /// longer approved.
    SchemaDeprecated,
    /// Returned by `proof_validity`: none of the above conditions hold.
    Valid,
}

/// One entry of a bounded batch registration request.
///
/// Mirrors the per-proof arguments of `register_proof` minus `issuer_address`,
/// since a batch registers proofs for a single authorized issuer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofRegistrationInput {
    pub proof_id_hash: BytesN<32>,
    pub commitment_hash: BytesN<32>,
    pub schema_version: u32,
    pub expires_at: u64,
}

/// Lifecycle state of a proof dispute. Terminal once `Withdrawn`, `Resolved`,
/// or `Rejected`: none of those transitions back to `Open`, and a new
/// dispute can only be opened once the previous one has reached one of them.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    /// Under review. The only status a proof may have at most one of at a
    /// time.
    Open,
    /// Withdrawn by whoever opened it, before any resolution.
    Withdrawn,
    /// Resolved by the admin in the disputant's favor.
    Resolved,
    /// Rejected by the admin as without merit.
    Rejected,
}

/// Coarse category of who took a dispute action, recorded alongside the
/// address itself so an indexer can distinguish "the issuer disputed their
/// own proof" from "a third party disputed it" without re-deriving it from
/// other contract state.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DisputeActorClass {
    /// The address is the proof's own recorded issuer.
    Issuer,
    /// The address is the proof-registry contract's admin.
    Admin,
    /// Any other address.
    ThirdParty,
}

/// Bounded, on-chain dispute state for one proof.
///
/// Deliberately does not store raw evidence or a free-form reason: only a
/// commitment hash to evidence held off-chain, mirroring how issuer-registry
/// records a `reason_commitment` rather than the reason text itself. Dispute
/// status is tracked independently of `ProofRecord.status`: opening,
/// resolving, or rejecting a dispute never changes a proof's validity or
/// revocation state, and revoking or expiring a proof never changes its
/// dispute state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    pub proof_id_hash: BytesN<32>,
    /// Hash of off-chain evidence. Never the evidence itself.
    pub evidence_commitment: BytesN<32>,
    pub status: DisputeStatus,
    pub opened_by: Address,
    pub opened_by_class: DisputeActorClass,
    pub opened_at: u64,
    /// Address that produced the current `status` — the opener while still
    /// `Open`, or whoever withdrew/resolved/rejected it.
    pub updated_by: Address,
    pub updated_by_class: DisputeActorClass,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaRecord {
    pub version: u32,
    pub metadata_hash: BytesN<32>,
    pub is_approved: bool,
    pub activated_at: u64,
    pub deprecated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeReceipt {
    pub wasm_hash: BytesN<32>,
    pub old_version: u32,
    pub new_version: u32,
    pub upgraded_at: u64,
    pub upgraded_by: Address,
}

/// Bounded, on-chain record of a proof that has been archived after
/// expiring or being revoked. Kept separate from `ProofRecord` storage so
/// live-proof lookups never have to filter out archived entries.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchivedProofRecord {
    pub proof_id_hash: BytesN<32>,
    pub commitment_hash: BytesN<32>,
    pub issuer_address: Address,
    pub was_revoked: bool,
    pub schema_version: u32,
    pub expired_at: u64,
    pub archived_at: u64,
}

/// On-chain record of an approved-but-not-yet-executed contract upgrade,
/// keyed by the approved WASM hash. Distinct from `UpgradeApprovalMetadata`,
/// which is the off-chain-facing query result derived from this record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeApprovalRecord {
    pub new_version: u32,
    pub target_contract: Address,
    pub contract_role: Symbol,
}

/// One entry in a contract's append-only upgrade history log.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeHistoryRecord {
    pub old_wasm_hash: BytesN<32>,
    pub new_wasm_hash: BytesN<32>,
    pub old_version: u32,
    pub new_version: u32,
    pub ledger_sequence: u32,
    pub ledger_timestamp: u64,
    pub upgraded_by: Address,
}
// ── Upgrade Approval Metadata ──────────────────────────────────────────────────
// Metadata for an upgrade approval, exposed for off-chain verification.
//
// This is the single shared structure used across all contracts that
// implement upgrade approval workflows. Generated clients see a consistent
// type regardless of which contract they interact with.
//
// # Off-chain verification use cases
// - Verify an upgrade plan matches the approved hash and version
// - Check the approval window (creation → expiry) to assess staleness
// - Confirm the execution ledger matches when approval was consumed
// - Audit which approver authorized the upgrade

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UpgradeApprovalMetadata {
    /// SHA-256 hash of the WASM bytecode approved for upgrade.
    /// Operators compare this against the upgrade package hash.
    pub target_hash: BytesN<32>,

    /// Semantic version string of the target contract version.
    /// Format: "MAJOR.MINOR.PATCH" (e.g. "1.2.0")
    pub target_version: u32,

    /// Address that submitted and signed this approval.
    pub approver: Address,

    /// Ledger sequence when the approval was created.
    pub creation_ledger: u32,

    /// Ledger sequence when the approved upgrade was executed.
    /// None if the approval has not yet been consumed.
    pub execution_ledger: Option<u32>,

    /// Ledger sequence after which this approval expires and cannot be used.
    pub expiry_ledger: u32,

    /// Current status of this approval.
    pub status: ApprovalStatus,
}

/// Unambiguous status for an upgrade approval.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ApprovalStatus {
    /// Approval is valid and within its window.
    Active,

    /// Approval was used — upgrade has been executed.
    Executed,

    /// Approval was explicitly revoked before execution.
    Revoked,

    /// Approval window has passed without execution.
    Expired,
}

/// Result of an approval metadata query.
/// Distinguishes "unknown" from "revoked" unambiguously.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ApprovalQuery {
    /// Approval exists — metadata included.
    Found(UpgradeApprovalMetadata),

    /// No approval record exists for this hash.
    /// Distinct from Revoked — the approval never existed or was pruned.
    NotFound,

    /// Approval existed but was explicitly revoked.
    /// Included metadata shows who approved and when, for audit purposes.
    Revoked(UpgradeApprovalMetadata),
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
pub struct ProofValidityDetails {
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

pub const MAX_SUCCESSORS: u32 = 5;

#[cfg(test)]
mod interface_version_tests {
    use super::*;

    const BASE: InterfaceVersion = InterfaceVersion::new(1, 2, 3);

    #[test]
    fn exact_match_is_compatible() {
        assert!(is_interface_compatible(&BASE, &BASE));
    }

    #[test]
    fn newer_patch_and_minor_within_major_are_compatible() {
        assert!(is_interface_compatible(
            &BASE,
            &InterfaceVersion::new(1, 2, 4)
        ));
        assert!(is_interface_compatible(
            &BASE,
            &InterfaceVersion::new(1, 3, 0)
        ));
        assert!(is_interface_compatible(
            &BASE,
            &InterfaceVersion::new(1, 9, 9)
        ));
    }

    #[test]
    fn older_minor_or_patch_is_incompatible() {
        assert!(!is_interface_compatible(
            &BASE,
            &InterfaceVersion::new(1, 2, 2)
        ));
        assert!(!is_interface_compatible(
            &BASE,
            &InterfaceVersion::new(1, 1, 9)
        ));
    }

    #[test]
    fn a_different_major_is_incompatible_in_both_directions() {
        assert!(!is_interface_compatible(
            &BASE,
            &InterfaceVersion::new(2, 0, 0)
        ));
        assert!(!is_interface_compatible(
            &BASE,
            &InterfaceVersion::new(0, 9, 9)
        ));
    }
}

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
