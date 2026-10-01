#![no_std]

use soroban_sdk::{
    contracterror, contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env, String, Symbol,
};

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
/// Maximum number of distinct signers in a critical-action approval policy.
pub const MAX_CRITICAL_ACTION_SIGNERS: u32 = 16;
/// Maximum lifetime of a critical-action proposal, in ledgers.
pub const CRITICAL_ACTION_APPROVAL_EXPIRY_LEDGERS: u32 = 518_400;
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

/// Maximum validity duration that protocol governance may assign to a schema.
pub const MAX_SCHEMA_VALIDITY_SECONDS: u64 = 3_153_600_000;
/// Maximum number of numeric proof types allowed in one schema policy.
pub const MAX_SCHEMA_PROOF_TYPES: u32 = 16;
/// Compatibility proof type used by legacy registration entry points.
pub const LEGACY_PROOF_TYPE: u32 = 0;
/// Stable commitment algorithm identifiers; zero retains the legacy behavior.
pub const LEGACY_COMMITMENT_ALGORITHM: u32 = 0;
pub const SHA256_COMMITMENT_ALGORITHM_V1: u32 = 1;

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

/// Per-schema registration policy. Proof-type identifiers are stable numeric
/// values defined by the integrating application; the legacy identifier `0`
/// is reserved for the original registration API.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaPolicy {
    pub proof_types: soroban_sdk::Vec<u32>,
    pub max_validity_seconds: u64,
}

/// Immutable registration metadata kept separately from `ProofRecord` so
/// existing persisted proof records remain decodable across upgrades.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofPolicySnapshot {
    pub proof_type: u32,
    pub commitment_algorithm: u32,
    pub max_validity_seconds: u64,
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
pub const PROTOCOL_CONFIG_INTERFACE_VERSION: InterfaceVersion = InterfaceVersion::new(1, 1, 0);

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GovernanceRole {
    ProtocolPause,
    SchemaManagement,
    IssuerManagement,
    ProofAdministration,
    DependencyManagement,
    UpgradeManagement,
    Recovery,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernanceRoleAssignment {
    pub role: GovernanceRole,
    pub address: Address,
    pub activation_ledger: u32,
    pub expiration_ledger: Option<u32>,
    pub proposal_id: BytesN<32>,
}

impl GovernanceRoleAssignment {
    pub fn is_active_at(&self, ledger: u32) -> bool {
        ledger >= self.activation_ledger
            && self
                .expiration_ledger
                .map(|expiration| ledger < expiration)
                .unwrap_or(true)
    }

    pub fn is_pending_at(&self, ledger: u32) -> bool {
        ledger < self.activation_ledger
    }
}

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
    /// A bounded batch query supplied more items than its documented maximum.
    BatchTooLarge = 64,
    /// A cross-contract dependency reported an interface version outside the
    /// range the consumer accepts.
    IncompatibleInterfaceVersion = 62,

    // Protocol state errors (80-99)
    ProtocolPaused = 80,

    // Upgrade timing errors (90-99)
    NoUpgradeApproval = 90,
    UpgradeTimelockNotElapsed = 91,
    UpgradeApprovalExpired = 92,
    WasmHashMismatch = 93,
    InvalidTimingConfig = 94,
    ThresholdApprovalRequired = 95,
    ApprovalProposalNotFound = 96,
    ApprovalProposalExpired = 97,
    InsufficientApprovals = 98,
    InvalidApprovalPolicy = 99,
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
    InvalidMetadataCommitment = 211,
    /// Registering or reactivating this issuer would exceed the governed
    /// maximum active-issuer capacity.
    IssuerCapacityExceeded = 208,
    /// A requested capacity limit is below the current active-issuer usage and
    /// no explicit override was supplied.
    MaxBelowActiveUsage = 209,
    /// The suspended issuer's reactivation cooldown has not yet elapsed.
    ReactivationCooldownActive = 210,
    /// A bounded batch issuer-status query exceeded its documented maximum.
    BatchTooLarge = 212,
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
    /// The proof-type identifier is unknown or deprecated in protocol config.
    UnsupportedProofType = 316,
    /// The network passphrase or asset identifier is not canonical, or the
    /// passphrase does not match the current ledger network.
    InvalidProofContext = 317,
}

/// Versioned asset identifier accepted by context-aware proof registration.
/// Issued asset codes are case-sensitive ASCII alphanumeric strings of 1-12
/// characters; the variant tag keeps native XLM distinct from an issued asset
/// whose code happens to be `XLM`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProofAssetIdentifier {
    Native,
    Issued(String, Address),
}

/// Public commitments that bind a proof claim to the network and asset policy
/// used by the backend. The raw network passphrase and asset identifier are
/// never stored in proof-registry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofContextCommitments {
    pub version: u32,
    pub network_commitment: BytesN<32>,
    pub asset_commitment: BytesN<32>,
    pub proof_context_commitment: BytesN<32>,
}

/// Context options supplied by an issuer during context-aware proof
/// registration. Raw passphrases and asset identifiers are not persisted.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofRegistrationContext {
    pub network_passphrase: String,
    pub asset: ProofAssetIdentifier,
    pub payload: Option<Bytes>,
    /// Fixed-size opaque commitment. All-zero bytes mean no pseudonym was
    /// supplied; non-zero values are stored as opaque bytes only.
    pub subject_pseudonym_commitment: BytesN<32>,
}

/// Computes version-1 network, asset, and claim-context commitments.
///
/// Network passphrases must be 1-128 visible ASCII bytes with no leading or
/// trailing spaces. Issued asset identifiers use the exact case-sensitive
/// ASCII code and a valid Stellar account address as issuer.
pub fn compute_proof_context_commitments(
    env: &Env,
    claim_commitment: &BytesN<32>,
    network_passphrase: &String,
    asset: &ProofAssetIdentifier,
) -> Option<ProofContextCommitments> {
    let network_bytes = network_passphrase.to_bytes();
    let network_len = network_bytes.len();
    if network_len == 0 || network_len > 128 {
        return None;
    }
    for index in 0..network_len {
        let byte = network_bytes.get(index)?;
        if !(0x20..=0x7e).contains(&byte)
            || (index == 0 || index == network_len - 1) && byte == b' '
        {
            return None;
        }
    }
    if env.crypto().sha256(&network_bytes).to_bytes() != env.ledger().network_id() {
        return None;
    }

    let mut network_preimage = Bytes::from_slice(env, b"earnproof.network.v1\0");
    network_preimage.append(&network_bytes);
    let network_commitment = env.crypto().sha256(&network_preimage).to_bytes();

    let mut asset_preimage = Bytes::from_slice(env, b"earnproof.asset.v1\0");
    match asset {
        ProofAssetIdentifier::Native => asset_preimage.append(&Bytes::from_slice(env, b"native")),
        ProofAssetIdentifier::Issued(code, issuer) => {
            let code_bytes = code.to_bytes();
            let code_len = code_bytes.len();
            if code_len == 0 || code_len > 12 || !is_valid_account_address(issuer) {
                return None;
            }
            for index in 0..code_len {
                let byte = code_bytes.get(index)?;
                if !byte.is_ascii_alphanumeric() {
                    return None;
                }
            }
            asset_preimage.append(&Bytes::from_slice(env, b"issued\0"));
            asset_preimage.append(&Bytes::from_array(env, &[code_len as u8]));
            asset_preimage.append(&code_bytes);
            asset_preimage.append(&issuer.to_string().to_bytes());
        }
    };
    let asset_commitment = env.crypto().sha256(&asset_preimage).to_bytes();

    let mut context_preimage = Bytes::from_slice(env, b"earnproof.proof-context.v1\0");
    context_preimage.append(&Bytes::from_slice(
        env,
        claim_commitment.to_array().as_slice(),
    ));
    context_preimage.append(&network_commitment.to_bytes());
    context_preimage.append(&asset_commitment.to_bytes());
    let proof_context_commitment = env.crypto().sha256(&context_preimage).to_bytes();

    Some(ProofContextCommitments {
        version: 1,
        network_commitment,
        asset_commitment,
        proof_context_commitment,
    })
}

/// Returns true for a canonical account address, excluding contract addresses
/// that cannot issue a classic Stellar asset.
pub fn is_valid_account_address(address: &Address) -> bool {
    is_valid_principal_address(address) && address.to_string().to_bytes().get(0) == Some(b'G')
}

/// Derives a storage identifier for a context-bound proof record. Reusing the
/// same caller claim ID with a different network or asset yields a distinct
/// record key.
pub fn derive_contextual_proof_id(
    env: &Env,
    claim_id: &BytesN<32>,
    context_commitment: &BytesN<32>,
) -> BytesN<32> {
    let mut preimage = Bytes::from_slice(env, b"earnproof.proof-record.v1\0");
    preimage.append(&claim_id.to_bytes());
    preimage.append(&context_commitment.to_bytes());
    env.crypto().sha256(&preimage).to_bytes()
}

/// Computes an issuer- and purpose-scoped commitment for a subject pseudonym.
/// The raw pseudonym is input only and is never included in a persisted record.
pub fn compute_subject_pseudonym_commitment(
    env: &Env,
    issuer_address: &Address,
    domain: &String,
    subject_pseudonym: &BytesN<32>,
) -> Option<BytesN<32>> {
    if !is_valid_account_address(issuer_address) {
        return None;
    }
    let domain_bytes = domain.to_bytes();
    let domain_len = domain_bytes.len();
    if domain_len == 0 || domain_len > 64 {
        return None;
    }
    for index in 0..domain_len {
        let byte = domain_bytes.get(index)?;
        if !(0x21..=0x7e).contains(&byte) {
            return None;
        }
    }

    let mut preimage = Bytes::from_slice(env, b"earnproof.subject-pseudonym.v1\0");
    preimage.append(&Bytes::from_array(env, &[domain_len as u8]));
    preimage.append(&domain_bytes);
    preimage.append(&issuer_address.to_string().to_bytes());
    preimage.append(&subject_pseudonym.to_bytes());
    Some(env.crypto().sha256(&preimage).to_bytes())
}

/// Interprets the all-zero commitment sentinel as explicit absence.
pub fn optional_subject_pseudonym_commitment(commitment: &BytesN<32>) -> Option<BytesN<32>> {
    if commitment.to_array() == [0; 32] {
        None
    } else {
        Some(commitment.clone())
    }
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
    SchemaPolicy,
    CommitmentAlgorithmPolicy,
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

/// Maximum identifiers accepted by one bounded issuer status query.
pub const MAX_ISSUER_STATUS_BATCH: u32 = 50;

/// Status returned for one identifier in a bounded issuer status query.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IssuerQueryStatus {
    Active,
    Suspended,
    Revoked,
    NotFound,
}

/// One issuer id and its current status in a bounded query response.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuerStatusResult {
    pub issuer_id_hash: BytesN<32>,
    pub status: IssuerQueryStatus,
}

/// Maximum schema versions accepted by one bounded status query.
pub const MAX_SCHEMA_STATUS_BATCH: u32 = 50;

/// Lifecycle state for one schema version in a batch query.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaVersionState {
    Unknown,
    Approved,
    Deprecated,
}

/// One version and its status in a bounded schema query response.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaStatusResult {
    pub version: u32,
    pub state: SchemaVersionState,
}

/// Maximum number of predecessor links a schema-lineage query may traverse.
pub const MAX_SCHEMA_LINEAGE_DEPTH: u32 = 32;

/// Canonical reason returned by the dependency-aware proof validity query.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProofValidityReason {
    Valid,
    Unknown,
    Revoked,
    Expired,
    IssuerInactive,
    SchemaDeprecated,
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
    pub disclosure_policy_hash: BytesN<32>,
    pub issuer_address: Address,
    pub status: ProofStatus,
    pub schema_version: u32,
    pub expires_at: u64,
    pub created_at: u64,
    pub revoked_at: u64,
    /// Stable protocol proof type. `None` is the explicit legacy marker for
    /// records written before proof types were committed to storage.
    pub proof_type: Option<BytesN<32>>,
    /// Ledger sequence at revocation; zero marks legacy records without it.
    pub revoked_ledger: u32,
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

/// Detailed timing metadata returned alongside a proof validity summary.
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
    pub proof_type: BytesN<32>,
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

// ── Shared Test Utilities ──────────────────────────────────────────────────────
// These utilities provide common patterns for initialization adversarial testing
// across all contracts, ensuring consistent test coverage for re-initialization
// guards, invalid dependencies, and state/event immutability on failure.

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
mod proof_context_tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Ledger as _, Env};

    #[contract]
    pub struct TestContextContract;

    #[contractimpl]
    impl TestContextContract {
        pub fn ping(_env: Env) {}
    }

    const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";
    const ISSUER: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";
    const OTHER_ISSUER: &str = "GDWUSKGGFDI4FRXK5EBTRECZSVQSSWJHHJOGH6JWG3AUMFFMQ435DIAG";

    fn bytes32(env: &Env, hex: &str) -> BytesN<32> {
        assert_eq!(hex.len(), 64);
        let mut bytes = [0_u8; 32];
        for (index, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            bytes[index] = u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        BytesN::from_array(env, &bytes)
    }

    fn vector_hex(id: &str) -> &'static str {
        include_str!("../../../tests/fixtures/encoding/vectors.tsv")
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .find_map(|line| {
                let fields: std::vec::Vec<&str> = line.split('\t').collect();
                (fields[0] == id).then_some(fields[3])
            })
            .unwrap_or_else(|| panic!("missing encoding vector {id}"))
    }

    #[test]
    fn cross_language_context_vectors_match() {
        let env = Env::default();
        let claim = bytes32(
            &env,
            "7261c38367d18cd03b133d7011956d1a8a35daf3e379aed2d45cdf33be235f35",
        );
        let claim_id = bytes32(
            &env,
            "c5aecb1a93a48d868c6708d746a71d7eb57f0cfd7a18f0659f97d34fc63efa19",
        );
        let passphrase = String::from_str(&env, TESTNET_PASSPHRASE);
        env.ledger()
            .set_network_id(env.crypto().sha256(&passphrase.to_bytes()).to_array());

        let native = compute_proof_context_commitments(
            &env,
            &claim,
            &passphrase,
            &ProofAssetIdentifier::Native,
        )
        .unwrap();
        assert_eq!(
            native.network_commitment,
            bytes32(&env, vector_hex("network-v1"))
        );
        assert_eq!(
            native.asset_commitment,
            bytes32(&env, vector_hex("asset-native-v1"))
        );
        assert_eq!(
            native.proof_context_commitment,
            bytes32(&env, vector_hex("context-native-v1"))
        );
        assert_eq!(
            derive_contextual_proof_id(&env, &claim_id, &native.proof_context_commitment),
            bytes32(&env, vector_hex("record-native-v1"))
        );

        let issuer = Address::from_str(&env, ISSUER);
        let issued = compute_proof_context_commitments(
            &env,
            &claim,
            &passphrase,
            &ProofAssetIdentifier::Issued(String::from_str(&env, "USDC"), issuer),
        )
        .unwrap();
        assert_eq!(
            issued.asset_commitment,
            bytes32(&env, vector_hex("asset-issued-usdc-v1"))
        );
        assert_eq!(
            issued.proof_context_commitment,
            bytes32(&env, vector_hex("context-issued-usdc-v1"))
        );
        assert_eq!(
            derive_contextual_proof_id(&env, &claim_id, &issued.proof_context_commitment),
            bytes32(&env, vector_hex("record-issued-usdc-v1"))
        );
        assert_ne!(
            native.proof_context_commitment,
            issued.proof_context_commitment
        );
    }

    #[test]
    fn malformed_or_wrong_network_context_is_rejected() {
        let env = Env::default();
        let claim = BytesN::from_array(&env, &[1; 32]);
        let passphrase = String::from_str(&env, TESTNET_PASSPHRASE);
        let bad_code = ProofAssetIdentifier::Issued(
            String::from_str(&env, "USDC-USD"),
            Address::from_str(&env, ISSUER),
        );
        assert!(compute_proof_context_commitments(&env, &claim, &passphrase, &bad_code).is_none());

        for value in [
            "",
            " Test SDF Network ; September 2015",
            "Test SDF Network ; September 2015 ",
            "Mainnet",
        ] {
            assert!(compute_proof_context_commitments(
                &env,
                &claim,
                &String::from_str(&env, value),
                &ProofAssetIdentifier::Native,
            )
            .is_none());
        }

        let contract_issuer = env.register(TestContextContract, ());
        let contract_asset =
            ProofAssetIdentifier::Issued(String::from_str(&env, "USDC"), contract_issuer);
        assert!(
            compute_proof_context_commitments(&env, &claim, &passphrase, &contract_asset).is_none()
        );
    }

    #[test]
    fn same_claim_is_bound_to_distinct_network_ids() {
        let env = Env::default();
        let claim = BytesN::from_array(&env, &[7; 32]);
        let claim_id = BytesN::from_array(&env, &[8; 32]);
        let testnet = String::from_str(&env, TESTNET_PASSPHRASE);
        let testnet_id = env.crypto().sha256(&testnet.to_bytes()).to_array();
        env.ledger().set_network_id(testnet_id);
        let testnet_context = compute_proof_context_commitments(
            &env,
            &claim,
            &testnet,
            &ProofAssetIdentifier::Native,
        )
        .unwrap();

        let public_passphrase =
            String::from_str(&env, "Public Global Stellar Network ; September 2015");
        let public_id = env
            .crypto()
            .sha256(&public_passphrase.to_bytes())
            .to_array();
        env.ledger().set_network_id(public_id);
        let public_context = compute_proof_context_commitments(
            &env,
            &claim,
            &public_passphrase,
            &ProofAssetIdentifier::Native,
        )
        .unwrap();

        assert_ne!(
            testnet_context.network_commitment,
            public_context.network_commitment
        );
        assert_ne!(
            testnet_context.proof_context_commitment,
            public_context.proof_context_commitment
        );
        assert_ne!(
            derive_contextual_proof_id(&env, &claim_id, &testnet_context.proof_context_commitment),
            derive_contextual_proof_id(&env, &claim_id, &public_context.proof_context_commitment)
        );
    }

    #[test]
    fn accepted_asset_code_and_network_length_boundaries() {
        let env = Env::default();
        let claim = BytesN::from_array(&env, &[9; 32]);
        let issuer = Address::from_str(&env, ISSUER);
        let network_text = "A".repeat(128);
        let network = String::from_str(&env, &network_text);
        env.ledger()
            .set_network_id(env.crypto().sha256(&network.to_bytes()).to_array());
        for code in ["A", "ABCDEFGHIJKL"] {
            let asset = ProofAssetIdentifier::Issued(String::from_str(&env, code), issuer.clone());
            assert!(compute_proof_context_commitments(&env, &claim, &network, &asset).is_some());
        }
        let too_long_code =
            ProofAssetIdentifier::Issued(String::from_str(&env, "ABCDEFGHIJKLM"), issuer);
        assert!(
            compute_proof_context_commitments(&env, &claim, &network, &too_long_code).is_none()
        );

        let too_long_network_text = "A".repeat(129);
        let too_long_network = String::from_str(&env, &too_long_network_text);
        env.ledger()
            .set_network_id(env.crypto().sha256(&too_long_network.to_bytes()).to_array());
        assert!(compute_proof_context_commitments(
            &env,
            &claim,
            &too_long_network,
            &ProofAssetIdentifier::Native,
        )
        .is_none());
    }

    #[test]
    fn subject_pseudonym_commitment_matches_vector_and_is_issuer_domain_scoped() {
        let env = Env::default();
        let pseudonym = bytes32(
            &env,
            "1111111111111111111111111111111111111111111111111111111111111111",
        );
        let issuer = Address::from_str(&env, ISSUER);
        let other_issuer = Address::from_str(&env, OTHER_ISSUER);
        let domain = String::from_str(&env, "credential-verification");
        let commitment =
            compute_subject_pseudonym_commitment(&env, &issuer, &domain, &pseudonym).unwrap();

        assert_eq!(
            commitment,
            bytes32(&env, vector_hex("subject-pseudonym-v1"))
        );
        assert_ne!(
            commitment,
            compute_subject_pseudonym_commitment(
                &env,
                &issuer,
                &String::from_str(&env, "analytics"),
                &pseudonym,
            )
            .unwrap()
        );
        assert_ne!(
            commitment,
            compute_subject_pseudonym_commitment(&env, &other_issuer, &domain, &pseudonym).unwrap()
        );
    }

    #[test]
    fn subject_pseudonym_domain_and_zero_sentinel_boundaries_are_explicit() {
        let env = Env::default();
        let issuer = Address::from_str(&env, ISSUER);
        let pseudonym = BytesN::from_array(&env, &[0x22; 32]);
        for value in ["", "contains spaces", " bad-edge", "bad-edge "] {
            assert!(compute_subject_pseudonym_commitment(
                &env,
                &issuer,
                &String::from_str(&env, value),
                &pseudonym,
            )
            .is_none());
        }
        let max_domain = String::from_str(&env, &"d".repeat(64));
        let too_long_domain = String::from_str(&env, &"d".repeat(65));
        assert!(
            compute_subject_pseudonym_commitment(&env, &issuer, &max_domain, &pseudonym).is_some()
        );
        assert!(
            compute_subject_pseudonym_commitment(&env, &issuer, &too_long_domain, &pseudonym)
                .is_none()
        );
        assert!(
            optional_subject_pseudonym_commitment(&BytesN::from_array(&env, &[0; 32])).is_none()
        );
        assert_eq!(
            optional_subject_pseudonym_commitment(&pseudonym),
            Some(pseudonym)
        );
        let contract_issuer = env.register(TestContextContract, ());
        assert!(compute_subject_pseudonym_commitment(
            &env,
            &contract_issuer,
            &String::from_str(&env, "credential-verification"),
            &BytesN::from_array(&env, &[0x33; 32]),
        )
        .is_none());
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
