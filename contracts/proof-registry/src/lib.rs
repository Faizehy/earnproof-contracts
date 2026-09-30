#![no_std]

#[allow(unused_imports)]
use earnproof_shared::{
    is_interface_compatible, ApprovalQuery, ApprovalStatus, ArchivedProofRecord, ContractError,
    DisputeActorClass, DisputeRecord, DisputeStatus, GenesisRecord, InterfaceVersion,
    MigrationStatus, PauseScope, ProofError, ProofPayloadRecord, ProofPolicySnapshot, ProofRecord,
    ProofRegistrationInput, ProofStatus, ProofValidity, ProofValidityDetails, SchemaPolicy,
    SchemaRateLimit, SchemaRateLimitUsage, TtlStatus, UpgradeApproval, UpgradeApprovalMetadata,
    UpgradeApprovalRecord, UpgradeHistoryRecord, UpgradeReceipt, LEGACY_COMMITMENT_ALGORITHM,
    LEGACY_PROOF_TYPE, MAX_MIGRATION_BATCH, MAX_PROOF_BATCH_SIZE, MAX_SCHEMA_VALIDITY_SECONDS,
    MIGRATION_STATUS_VERSION, TTL_EXTEND_TO_LEDGERS, TTL_THRESHOLD_LEDGERS,
    UPGRADE_APPROVAL_EXPIRY_LEDGERS, UPGRADE_TIMELOCK_LEDGERS,
};
use soroban_sdk::{
    contract, contractclient, contractevent, contractimpl, contracttype, xdr::ToXdr, Address,
    Bytes, BytesN, Env, Symbol, Vec,
};

/// Minimum `issuer-registry` interface version this contract can bind to.
/// A dependency must report the same major and at least this minor/patch.
const REQUIRED_ISSUER_REGISTRY_VERSION: InterfaceVersion = InterfaceVersion::new(1, 0, 0);

/// Minimum `protocol-config` interface version this contract can bind to.
const REQUIRED_PROTOCOL_CONFIG_VERSION: InterfaceVersion = InterfaceVersion::new(1, 0, 0);
const MAX_DEPENDENCY_PROPOSAL_LEDGERS: u32 = 518_400;

#[contractclient(name = "ProtocolConfigContractClient")]
pub trait ProtocolConfigInterface {
    fn is_paused(env: Env) -> bool;
    fn is_schema_version_approved(env: Env, version: u32) -> bool;
    fn is_proof_type_approved(env: Env, proof_type: BytesN<32>) -> bool;
    fn get_schema_policy(env: Env, version: u32) -> SchemaPolicy;
    fn is_algorithm_supported(env: Env, algorithm: u32) -> bool;
    fn get_schema_payload_limit(env: Env, version: u32) -> u32;
    fn get_schema_rate_limit(env: Env, version: u32) -> SchemaRateLimit;
    fn interface_version(env: Env) -> InterfaceVersion;
}

#[contractclient(name = "IssuerRegistryContractClient")]
pub trait IssuerRegistryInterface {
    fn is_active_address(env: Env, issuer_address: Address) -> bool;
    fn interface_version(env: Env) -> InterfaceVersion;
}

#[contract]
pub struct ProofRegistryContract;

#[contracttype]
enum DataKey {
    MigrationStatus,
    Admin,
    IssuerRegistry,
    ProtocolConfig,
    Proof(BytesN<32>),
    /// Immutable numeric schema policy selected when the proof was registered.
    ProofPolicy(BytesN<32>),
    RevocationInfo(BytesN<32>),
    ExecutedProposal(BytesN<32>),
    /// Allowlist entry: maps a WASM hash to the target contract version.
    AllowedWasm(BytesN<32>),
    /// Monotonically-increasing contract version.  Prevents downgrade.
    ContractVersion,
    Successor,
    Decommissioned,
    /// Immutable deployment identity, written once at `initialize`.
    Genesis,
    /// Monotonic epoch, advanced once per externally visible proof mutation
    /// (registration, revocation, supersession, or archival change).
    RegistryEpoch,
    /// Bounded auxiliary-payload metadata for a proof registered with a
    /// payload (length and commitment hash only — never the raw bytes).
    ProofPayloadMeta(BytesN<32>),
    SchemaRateUsage(u32, u32),
    IssuerProofCaps(Address),
    IssuerActiveProofCount(Address),
    IssuerLifetimeProofCount(Address),
    PendingAdmin,
    /// Per-scope pause flag (issue: pause_scope/unpause_scope).
    ScopedPause(PauseScope),
    /// Active timelocked upgrade approval (single-slot, instance storage).
    UpgradeApproval,
    /// Off-chain-verifiable upgrade approval metadata, keyed by target hash.
    UpgradeApprovalMetadata(BytesN<32>),
    /// WASM hash currently installed, tracked for upgrade history entries.
    CurrentWasmHash,
    /// Most recent completed upgrade's receipt.
    LatestUpgradeReceipt,
    /// Number of entries recorded in the upgrade history log.
    UpgradeHistoryCount,
    /// One upgrade history log entry, keyed by its index.
    UpgradeHistory(u32),
}

#[contractevent]
pub struct AdminTransferNominated {
    pub pending_admin: Address,
    pub nominated_by: Address,
}

#[contractevent]
pub struct AdminTransferAccepted {
    pub new_admin: Address,
}

#[contractevent]
pub struct AdminTransferCancelled {
    pub pending_admin: Address,
    pub cancelled_by: Address,
}

#[contractevent]
pub struct DependencyReplacementProposed {
    pub proposal_id: BytesN<32>,
    pub issuer_registry: Address,
    pub protocol_config: Address,
    pub expires_at_ledger: u32,
    pub proposed_by: Address,
}

#[contractevent]
pub struct DependencyReplacementCancelled {
    pub proposal_id: BytesN<32>,
    pub cancelled_by: Address,
}

#[contractevent]
pub struct DependenciesReplaced {
    pub proposal_id: BytesN<32>,
    pub previous_issuer_registry: Address,
    pub issuer_registry: Address,
    pub previous_protocol_config: Address,
    pub protocol_config: Address,
    pub activated_by: Address,
}

#[contractevent]
pub struct GovernanceRoleGranted {
    pub assignment: GovernanceRoleAssignment,
    pub granted_by: Address,
}

#[contractevent]
pub struct GovernanceRoleRemoved {
    pub role: GovernanceRole,
    pub address: Address,
    pub removed_by: Address,
}

// ── upgrade events ────────────────────────────────────────────────────────────

/// Emitted when the admin adds a WASM hash to the upgrade allowlist.
#[contractevent]
pub struct UpgradeAllowlisted {
    pub proposal_id: BytesN<32>,
    pub wasm_hash: BytesN<32>,
    pub new_contract_version: u32,
    pub approved_by: Address,
}

/// Emitted when the admin removes a WASM hash from the allowlist without
/// applying it.
#[contractevent]
pub struct UpgradeRevoked {
    pub proposal_id: BytesN<32>,
    pub wasm_hash: BytesN<32>,
    pub revoked_by: Address,
}

/// Emitted when a WASM upgrade is successfully applied.
#[contractevent]
pub struct ContractUpgraded {
    pub new_wasm_hash: BytesN<32>,
    pub old_contract_version: u32,
    pub new_contract_version: u32,
    pub upgraded_by: Address,
}

/// Emitted when an admin pauses or unpauses a specific operational scope.
#[contractevent]
pub struct ScopedPauseChanged {
    pub scope: PauseScope,
    pub paused: bool,
    pub changed_by: Address,
}

/// Emitted when a proof (expired or revoked) is moved to archival storage.
#[contractevent]
pub struct ProofArchived {
    pub proof_id_hash: BytesN<32>,
    pub archived_at: u64,
}

#[contractevent]
pub struct SuccessorNominated {
    pub proposal_id: BytesN<32>,
    pub successor: Address,
    pub nominated_by: Address,
}

#[contractevent]
pub struct ContractDecommissioned {
    pub proposal_id: BytesN<32>,
    pub old_instance: Address,
    pub successor_instance: Address,
    pub activated_by: Address,
}

// ── registry epoch events ────────────────────────────────────────────────────
//
// Every externally visible proof mutation carries the registry epoch it
// advanced to, so a cache or indexer can invalidate on the epoch alone
// instead of diffing individual records. Each entry point below still
// publishes exactly one event, matching the "at most one event per
// invocation" invariant documented in docs/events.md.

/// Emitted when a proof is registered without an auxiliary payload.
#[contractevent]
pub struct ProofRegistered {
    pub proof_id_hash: BytesN<32>,
    pub issuer_address: Address,
    pub schema_version: u32,
    pub created_ledger: u32,
    pub created_at: u64,
    pub expires_at: u64,
    pub disclosure_policy_hash: BytesN<32>,
    pub proof_type: u32,
    pub commitment_algorithm: u32,
    pub epoch: u32,
}

/// Emitted when a proof is registered with an auxiliary payload.
#[contractevent]
pub struct ProofRegisteredWithPayload {
    pub proof_id_hash: BytesN<32>,
    pub issuer_address: Address,
    pub schema_version: u32,
    pub created_ledger: u32,
    pub created_at: u64,
    pub expires_at: u64,
    pub disclosure_policy_hash: BytesN<32>,
    pub proof_type: u32,
    pub commitment_algorithm: u32,
    pub payload_len: u32,
    pub payload_hash: BytesN<32>,
    pub epoch: u32,
}

/// Emitted when a proof is revoked, by its issuer or by the admin.
#[contractevent]
pub struct ProofRevoked {
    pub proof_id_hash: BytesN<32>,
    pub revoked_at: u64,
    pub revoked_ledger: u32,
    pub by_admin: bool,
    pub epoch: u32,
}

#[contractimpl]
impl ProofRegistryContract {
    pub fn is_decommissioned(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Decommissioned)
            .unwrap_or(false)
    }

    fn ensure_not_decommissioned(env: &Env) -> Result<(), ProofError> {
        if Self::is_decommissioned(env.clone()) {
            Err(ProofError::ProofNotFound)
        } else {
            Ok(())
        }
    }

    pub fn initialize(
        env: Env,
        admin: Address,
        issuer_registry: Address,
        protocol_config: Address,
    ) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }

        Self::require_valid_principal(&admin)?;
        Self::validate_dependency_addresses(&env, &issuer_registry, &protocol_config)?;
        // Reject dependencies whose interface this contract does not understand
        // before any state is written, so a failed handshake mutates nothing.
        Self::require_compatible_dependencies(&env, &issuer_registry, &protocol_config)?;
        Self::require_auth(&admin);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::IssuerRegistry, &issuer_registry);
        env.storage()
            .instance()
            .set(&DataKey::ProtocolConfig, &protocol_config);
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &1_u32);
        env.storage()
            .instance()
            .set(&DataKey::RegistryEpoch, &0_u32);
        let genesis = GenesisRecord {
            genesis_id: earnproof_shared::compute_genesis_id(&env, "earnproof_proof_registry"),
            initialized_at_ledger: env.ledger().sequence(),
        };
        env.storage().instance().set(&DataKey::Genesis, &genesis);
        Self::extend_instance_ttl(env);
        Ok(())
    }

    /// Returns the immutable genesis identity recorded at `initialize`.
    /// Unchanged across upgrades and storage migrations.
    pub fn get_genesis(env: Env) -> Result {
        env.storage()
            .instance()
            .get(&DataKey::Genesis)
            .ok_or(ContractError::NotInitialized)
    }

    /// Returns the current registry epoch: a monotonic counter advanced once
    /// per externally visible proof mutation (registration, revocation,
    /// supersession, or archival change). Read-only calls and failed writes
    /// never advance it, and it survives upgrades and migrations because it
    /// lives in the same instance storage they do not touch.
    pub fn get_registry_epoch(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RegistryEpoch)
            .unwrap_or(0)
    }

    /// Returns the bounded auxiliary-payload metadata recorded for a proof
    /// registered via `register_proof_with_payload`.
    pub fn get_proof_payload(env: Env, proof_id_hash: BytesN<32>) -> Result {
        let key = DataKey::ProofPayloadMeta(proof_id_hash);
        let record = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ProofError::ProofNotFound)?;
        Self::extend_payload_key_ttl(env, &key);
        Ok(record)
    }

    pub fn nominate_successor(
        env: Env,
        proposal_id: BytesN<32>,
        successor: Address,
    ) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        Self::require_valid_issuer_address(&successor)?;
        Self::require_auth(&admin);
        Self::consume_proposal(&env, &proposal_id).map_err(|_| ProofError::ProofNotFound)?;
        env.storage()
            .instance()
            .set(&DataKey::Successor, &successor);
        SuccessorNominated {
            proposal_id,
            successor: successor.clone(),
            nominated_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_successor(env: Env) -> Option {
        env.storage().instance().get(&DataKey::Successor)
    }

    pub fn activate_successor(env: Env, proposal_id: BytesN<32>) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        Self::require_auth(&admin);
        let successor: Address = env
            .storage()
            .instance()
            .get(&DataKey::Successor)
            .ok_or(ProofError::ProofNotFound)?;

        Self::consume_proposal(&env, &proposal_id).map_err(|_| ProofError::ProofNotFound)?;
        env.storage()
            .instance()
            .set(&DataKey::Decommissioned, &true);
        ContractDecommissioned {
            proposal_id,
            old_instance: env.current_contract_address(),
            successor_instance: successor,
            activated_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn keepalive_instance(env: Env) -> bool {
        if !env.storage().instance().has(&DataKey::Admin) {
            return false;
        }
        Self::extend_instance_ttl(env);
        true
    }

    pub fn keepalive_proof(env: Env, proof_id_hash: BytesN<32>) -> bool {
        let key = DataKey::Proof(proof_id_hash.clone());
        if env.storage().persistent().has(&key) {
            Self::extend_proof_key_ttl(env.clone(), &key);
            Self::extend_proof_policy_ttl(env, &proof_id_hash);
            true
        } else {
            false
        }
    }

    pub fn pause_scope(env: Env, scope: PauseScope) -> Result<(), ProofError> {
        let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        Self::require_auth(&admin);
        env.storage()
            .persistent()
            .set(&DataKey::ScopedPause(scope), &true);
        env.storage().persistent().extend_ttl(
            &DataKey::ScopedPause(scope),
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        ScopedPauseChanged {
            scope,
            paused: true,
            changed_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn unpause_scope(env: Env, scope: PauseScope) -> Result<(), ProofError> {
        let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        Self::require_auth(&admin);
        env.storage()
            .persistent()
            .set(&DataKey::ScopedPause(scope), &false);
        env.storage().persistent().extend_ttl(
            &DataKey::ScopedPause(scope),
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        ScopedPauseChanged {
            scope,
            paused: false,
            changed_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    /// Compatibility registration entry point. Records written through this
    /// method use the explicit legacy identifier rule (`proof_type: None`).
    pub fn register_proof(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
    ) -> Result<(), ProofError> {
        let legacy_disclosure_hash = BytesN::from_array(&env, &[0; 32]);
        Self::register_proof_internal(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            None,
            LEGACY_PROOF_TYPE,
            LEGACY_COMMITMENT_ALGORITHM,
            None,
            legacy_disclosure_hash,
            0,
        )
    }

    /// Registers a proof with the stable identifier approved by protocol
    /// governance. The identifier is checked before any persistent write.
    pub fn register_proof_with_type_identifier(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        proof_type_identifier: BytesN<32>,
    ) -> Result<(), ProofError> {
        let legacy_disclosure_hash = BytesN::from_array(&env, &[0; 32]);
        Self::register_proof_internal(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            Some(proof_type_identifier),
            LEGACY_PROOF_TYPE,
            LEGACY_COMMITMENT_ALGORITHM,
            None,
            legacy_disclosure_hash,
            0,
        )
    }

    /// Registers a proof using the governed numeric proof type and commitment
    /// algorithm. `register_proof` remains the legacy-compatible entry point.
    pub fn register_proof_with_policy(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        proof_type: u32,
        commitment_algorithm: u32,
    ) -> Result<(), ProofError> {
        let legacy_disclosure_hash = BytesN::from_array(&env, &[0; 32]);
        Self::register_proof_internal(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            None,
            proof_type,
            commitment_algorithm,
            None,
            legacy_disclosure_hash,
            0,
        )
    }

    /// Legacy-compatible registration with bounded auxiliary payload metadata.
    pub fn register_proof_with_payload(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        payload: Bytes,
    ) -> Result<(), ProofError> {
        let legacy_disclosure_hash = BytesN::from_array(&env, &[0; 32]);
        Self::register_proof_internal(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            None,
            LEGACY_PROOF_TYPE,
            LEGACY_COMMITMENT_ALGORITHM,
            Some(payload),
            legacy_disclosure_hash,
            0,
        )
    }

    /// Registers a proof with a stable type identifier and bounded payload.
    pub fn register_proof_with_type_identifier_and_payload(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        proof_type_identifier: BytesN<32>,
        payload: Bytes,
    ) -> Result<(), ProofError> {
        let legacy_disclosure_hash = BytesN::from_array(&env, &[0; 32]);
        Self::register_proof_internal(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            Some(proof_type_identifier),
            LEGACY_PROOF_TYPE,
            LEGACY_COMMITMENT_ALGORITHM,
            Some(payload),
            legacy_disclosure_hash,
            0,
        )
    }

    /// Registers a proof with governed numeric policy identifiers and payload.
    pub fn register_policy_payload(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        proof_type: u32,
        commitment_algorithm: u32,
        payload: Bytes,
    ) -> Result<(), ProofError> {
        let legacy_disclosure_hash = BytesN::from_array(&env, &[0; 32]);
        Self::register_proof_internal(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            None,
            proof_type,
            commitment_algorithm,
            Some(payload),
            legacy_disclosure_hash,
            0,
        )
    }

    /// Records a nonzero disclosure-policy commitment with legacy numeric IDs.
    pub fn register_proof_payload_policy(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        disclosure_policy_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        payload: Bytes,
    ) -> Result<(), ProofError> {
        if disclosure_policy_hash == BytesN::from_array(&env, &[0; 32]) {
            return Err(ProofError::MalformedInput);
        }
        Self::register_proof_internal(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            None,
            LEGACY_PROOF_TYPE,
            LEGACY_COMMITMENT_ALGORITHM,
            Some(payload),
            disclosure_policy_hash,
            0,
        )
    }

    /// Registers a proof with a fixed future activation timestamp.
    pub fn register_proof_with_activation(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        activates_at: u64,
        proof_type_identifier: BytesN<32>,
    ) -> Result<(), ProofError> {
        let legacy_disclosure_hash = BytesN::from_array(&env, &[0; 32]);
        Self::register_proof_internal(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            Some(proof_type_identifier),
            LEGACY_PROOF_TYPE,
            LEGACY_COMMITMENT_ALGORITHM,
            None,
            legacy_disclosure_hash,
            activates_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn register_proof_internal(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        proof_type_identifier: Option<BytesN<32>>,
        proof_type: u32,
        commitment_algorithm: u32,
        payload: Option<Bytes>,
        disclosure_policy_hash: BytesN<32>,
        activates_at: u64,
    ) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        Self::require_valid_issuer_address(&issuer_address)?;
        let protocol_config =
            Self::get_protocol_config(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        let issuer_registry =
            Self::get_issuer_registry(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        if issuer_address == env.current_contract_address()
            || issuer_address == protocol_config
            || issuer_address == issuer_registry
        {
            return Err(ProofError::InvalidAddress);
        }
        Self::require_auth(&issuer_address);

        if schema_version == 0 {
            return Err(ProofError::InvalidSchemaVersion);
        }
        let now = env.ledger().timestamp();
        if expires_at <= now {
            return Err(ProofError::ProofExpired);
        }
        if activates_at >= expires_at {
            return Err(ProofError::InvalidActivationTime);
        }

        let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_config);
        if protocol_client.is_paused() {
            return Err(ProofError::ContractPaused);
        }
        let issuer_client = IssuerRegistryContractClient::new(&env, &issuer_registry);
        if !issuer_client.is_active_address(&issuer_address) {
            return Err(ProofError::IssuerInactive);
        }
        if !protocol_client.is_schema_version_approved(&schema_version) {
            return Err(ProofError::UnsupportedSchema);
        }

        // Validate both protocol policy layers and all caller-provided bounds
        // before consuming limits or writing any storage.
        if let Some(identifier) = &proof_type_identifier {
            if !protocol_client.is_proof_type_approved(identifier) {
                return Err(ProofError::UnsupportedProofType);
            }
        }
        let policy = protocol_client.get_schema_policy(&schema_version);
        if !Self::schema_allows_proof_type(&policy, proof_type)
            || !protocol_client.is_algorithm_supported(&commitment_algorithm)
        {
            return Err(ProofError::MalformedInput);
        }
        let validity = expires_at
            .checked_sub(now)
            .ok_or(ProofError::ProofExpired)?;
        if validity == 0 || validity > policy.max_validity_seconds {
            return Err(ProofError::ProofExpired);
        }
        let payload_len = if let Some(payload) = &payload {
            let length = payload.len();
            if length > protocol_client.get_schema_payload_limit(&schema_version) {
                return Err(ProofError::MalformedInput);
            }
            Some(length)
        } else {
            None
        };

        let key = DataKey::Proof(proof_id_hash.clone());
        if env.storage().persistent().has(&key) {
            return Err(ProofError::ProofAlreadyRegistered);
        }
        Self::consume_schema_rate_limit(&env, &protocol_client, schema_version)?;
        Self::consume_issuer_proof_capacity(&env, &issuer_address)?;

        let created_ledger = env.ledger().sequence();
        let record = ProofRecord {
            proof_id_hash: proof_id_hash.clone(),
            commitment_hash,
            disclosure_policy_hash: disclosure_policy_hash.clone(),
            issuer_address: issuer_address.clone(),
            status: ProofStatus::Active,
            schema_version,
            expires_at,
            created_at: now,
            revoked_at: 0,
            proof_type: proof_type_identifier,
            revoked_ledger: 0,
            created_ledger,
            activates_at,
        };
        env.storage().persistent().set(&key, &record);
        Self::extend_proof_key_ttl(env.clone(), &key);
        let policy_key = DataKey::ProofPolicy(proof_id_hash.clone());
        env.storage().persistent().set(
            &policy_key,
            &ProofPolicySnapshot {
                proof_type,
                commitment_algorithm,
                max_validity_seconds: policy.max_validity_seconds,
            },
        );
        Self::extend_proof_key_ttl(env.clone(), &policy_key);

        let epoch = Self::bump_registry_epoch(&env);
        if let (Some(payload), Some(payload_len)) = (payload, payload_len) {
            let payload_hash = env.crypto().sha256(&payload).to_bytes();
            let payload_key = DataKey::ProofPayloadMeta(proof_id_hash.clone());
            env.storage().persistent().set(
                &payload_key,
                &ProofPayloadRecord {
                    payload_len,
                    payload_hash: payload_hash.clone(),
                },
            );
            Self::extend_payload_key_ttl(env.clone(), &payload_key);
            ProofRegisteredWithPayload {
                proof_id_hash,
                issuer_address,
                schema_version,
                created_ledger,
                created_at: now,
                expires_at,
                disclosure_policy_hash,
                proof_type,
                commitment_algorithm,
                payload_len,
                payload_hash,
                epoch,
            }
            .publish(&env);
        } else {
            ProofRegistered {
                proof_id_hash,
                issuer_address,
                schema_version,
                created_ledger,
                created_at: now,
                expires_at,
                disclosure_policy_hash,
                proof_type,
                commitment_algorithm,
                epoch,
            }
            .publish(&env);
        }
        Ok(())
    }

    fn schema_allows_proof_type(policy: &SchemaPolicy, proof_type: u32) -> bool {
        let mut index = 0;
        while index < policy.proof_types.len() {
            if policy.proof_types.get(index) == Some(proof_type) {
                return true;
            }
            index += 1;
        }
        false
    }

    pub fn register_proofs_batch(
        env: Env,
        entries: Vec<ProofRegistrationInput>,
        issuer_address: Address,
    ) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        Self::require_valid_issuer_address(&issuer_address)?;

        let len = entries.len();
        if len == 0 || len > MAX_PROOF_BATCH_SIZE {
            return Err(ProofError::InvalidBatchSize);
        }

        let protocol_config =
            Self::get_protocol_config(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        let issuer_registry =
            Self::get_issuer_registry(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        if issuer_address == env.current_contract_address()
            || issuer_address == protocol_config
            || issuer_address == issuer_registry
        {
            return Err(ProofError::InvalidAddress);
        }
        Self::require_auth(&issuer_address);

        // Check 1: Contract paused (highest precedence — most external state)
        let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_config);
        if protocol_client.is_paused() {
            return Err(ProofError::ContractPaused);
        }

        // Check 2: Issuer active (issuer-specific state) — checked once for
        // the whole batch, since every entry shares the same issuer.
        let issuer_client = IssuerRegistryContractClient::new(&env, &issuer_registry);
        if !issuer_client.is_active_address(&issuer_address) {
            return Err(ProofError::IssuerInactive);
        }

        // Check all proof types before this batch mutates storage. This keeps
        // an unsupported type from partially consuming rate or issuer limits.
        for entry in entries.iter() {
            if !protocol_client.is_proof_type_approved(&entry.proof_type) {
                return Err(ProofError::UnsupportedProofType);
            }
        }

        let now = env.ledger().timestamp();
        let created_ledger = env.ledger().sequence();

        for entry in entries.iter() {
            if entry.schema_version == 0 {
                return Err(ProofError::InvalidSchemaVersion);
            }

            if entry.expires_at <= now {
                return Err(ProofError::ProofExpired);
            }

            // Check 3: Schema supported (protocol configuration state)
            if !protocol_client.is_schema_version_approved(&entry.schema_version) {
                return Err(ProofError::UnsupportedSchema);
            }
            let policy = protocol_client.get_schema_policy(&entry.schema_version);
            if !Self::schema_allows_proof_type(&policy, LEGACY_PROOF_TYPE)
                || !protocol_client.is_algorithm_supported(&LEGACY_COMMITMENT_ALGORITHM)
            {
                return Err(ProofError::MalformedInput);
            }
            let validity = entry
                .expires_at
                .checked_sub(now)
                .ok_or(ProofError::ProofExpired)?;
            if validity == 0 || validity > policy.max_validity_seconds {
                return Err(ProofError::ProofExpired);
            }

            // Check 4: Uniqueness constraint (storage precondition). A proof
            // id repeated earlier in this same batch is already visible here,
            // since writes made earlier in this call are readable within it —
            // so this single check also rejects intra-batch duplicates.
            let key = DataKey::Proof(entry.proof_id_hash.clone());
            if env.storage().persistent().has(&key) {
                return Err(ProofError::ProofAlreadyRegistered);
            }

            Self::consume_schema_rate_limit(&env, &protocol_client, entry.schema_version)?;
            Self::consume_issuer_proof_capacity(&env, &issuer_address)?;

            let record = ProofRecord {
                proof_id_hash: entry.proof_id_hash.clone(),
                commitment_hash: entry.commitment_hash.clone(),
                disclosure_policy_hash: BytesN::from_array(&env, &[0; 32]),
                issuer_address: issuer_address.clone(),
                status: ProofStatus::Active,
                schema_version: entry.schema_version,
                expires_at: entry.expires_at,
                created_at: now,
                revoked_at: 0,
                proof_type: Some(entry.proof_type.clone()),
                revoked_ledger: 0,
                created_ledger,
                activates_at: 0,
            };

            env.storage().persistent().set(&key, &record);
            let policy_key = DataKey::ProofPolicy(entry.proof_id_hash.clone());
            env.storage().persistent().set(
                &policy_key,
                &ProofPolicySnapshot {
                    proof_type: LEGACY_PROOF_TYPE,
                    commitment_algorithm: LEGACY_COMMITMENT_ALGORITHM,
                    max_validity_seconds: policy.max_validity_seconds,
                },
            );
            // A plain TTL bump, deliberately skipping the per-key TTL
            // tracker write `extend_proof_key_ttl` also performs: a batch of
            // up to `MAX_PROOF_BATCH_SIZE` entries must stay within the
            // per-invocation storage-write budget, and `get_proof_ttl_status`
            // is documented to reflect only calls made outside a batch.
            env.storage().persistent().extend_ttl(
                &key,
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );
            env.storage().persistent().extend_ttl(
                &policy_key,
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );

            ProofRegisteredInBatch {
                proof_id_hash: entry.proof_id_hash.clone(),
                issuer_address: issuer_address.clone(),
            }
            .publish(&env);
        }

        Ok(())
    }

    pub fn revoke_proof(env: Env, proof_id_hash: BytesN<32>) -> Result<(), ProofError> {
        Self::set_revoked(env, proof_id_hash, false)
    }

    pub fn admin_revoke_proof(env: Env, proof_id_hash: BytesN<32>) -> Result<(), ProofError> {
        Self::set_revoked(env, proof_id_hash, true)
    }

    pub fn admin_revoke_proof_by_role(
        env: Env,
        proof_id_hash: BytesN<32>,
        actor: Address,
    ) -> Result<(), ProofError> {
        Self::set_revoked_as(env, proof_id_hash, true, None, Some(actor))
    }

    pub fn revoke_proof_with_reason(
        env: Env,
        proof_id_hash: BytesN<32>,
        reason_commitment: BytesN<32>,
    ) -> Result<(), ProofError> {
        Self::set_revoked(env, proof_id_hash, false, Some(reason_commitment))
    }

    pub fn admin_revoke_proof_with_reason(
        env: Env,
        proof_id_hash: BytesN<32>,
        reason_commitment: BytesN<32>,
    ) -> Result<(), ProofError> {
        Self::set_revoked(env, proof_id_hash, true, Some(reason_commitment))
    }

    pub fn get_revocation_info(
        env: Env,
        proof_id_hash: BytesN<32>,
    ) -> Result<RevocationRecord, ProofError> {
        let key = DataKey::RevocationInfo(proof_id_hash);
        let record = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ProofError::ProofNotFound)?;
        Self::extend_proof_key_ttl(env.clone(), &key);
        Ok(record)
    }

    pub fn get_domain_separator(env: Env) -> BytesN<32> {
        shared_compute_domain_separator(
            &env,
            &env.ledger().network_id(),
            &env.current_contract_address(),
        )
    }

    pub fn compute_domain_commitment(env: Env, raw_commitment: BytesN<32>) -> BytesN<32> {
        let domain_sep = Self::get_domain_separator(env.clone());
        shared_compute_domain_commitment(&env, &domain_sep, &raw_commitment)
    }

    pub fn register_proof_with_domain(
        env: Env,
        proof_id_hash: BytesN<32>,
        raw_commitment: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
    ) -> Result<(), ProofError> {
        let commitment_hash = Self::compute_domain_commitment(env.clone(), raw_commitment);
        Self::register_proof(
            env,
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
        )
    }

    pub fn verify_domain_commitment(
        env: Env,
        proof_id_hash: BytesN<32>,
        raw_commitment: BytesN<32>,
    ) -> bool {
        match Self::get_proof(env.clone(), proof_id_hash) {
            Ok(record) => {
                let expected = Self::compute_domain_commitment(env, raw_commitment);
                record.commitment_hash == expected
            }
            Err(_) => false,
        }
    }

    pub fn is_valid_proof_batch(
        env: Env,
        proof_ids: soroban_sdk::Vec<BytesN<32>>,
    ) -> Result<soroban_sdk::Vec<bool>, ContractError> {
        if proof_ids.len() > MAX_PROOF_BATCH_SIZE {
            return Err(ContractError::InvalidInput);
        }
        let mut results = soroban_sdk::Vec::new(&env);
        for proof_id in proof_ids.iter() {
            results.push_back(Self::is_valid_proof(env.clone(), proof_id));
        }
        Ok(results)
    }

    pub fn get_proof(env: Env, proof_id_hash: BytesN<32>) -> Result<ProofRecord, ProofError> {
        let key = DataKey::Proof(proof_id_hash);
        let record: ProofRecord = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ProofError::ProofNotFound)?;
        Self::extend_proof_key_ttl(env.clone(), &key);
        Self::extend_proof_policy_ttl(env, &record.proof_id_hash);
        Ok(record)
    }

    /// Returns the immutable policy interpretation selected when the proof
    /// was registered. Older records are reported using explicit legacy IDs.
    pub fn get_proof_policy_snapshot(env: Env, proof_id_hash: BytesN<32>) -> Result {
        let proof_key = DataKey::Proof(proof_id_hash.clone());
        if !env.storage().persistent().has(&proof_key) {
            return Err(ProofError::ProofNotFound);
        }
        let key = DataKey::ProofPolicy(proof_id_hash.clone());
        let snapshot = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(ProofPolicySnapshot {
                proof_type: LEGACY_PROOF_TYPE,
                commitment_algorithm: LEGACY_COMMITMENT_ALGORITHM,
                max_validity_seconds: MAX_SCHEMA_VALIDITY_SECONDS,
            });
        Self::extend_proof_policy_ttl(env, &proof_id_hash);
        Ok(snapshot)
    }

    /// Legacy boolean validity helper, retained for compatibility.
    ///
    /// It reflects only the two locally-checkable conditions (status and
    /// expiry). For the full, structured reason — including issuer-inactive
    /// and deprecated-schema outcomes that require cross-contract reads — use
    /// [`Self::proof_validity`]. `is_valid_proof` returns `true` exactly when
    /// `proof_validity` would return one of `Valid`, `IssuerInactive`, or
    /// `SchemaDeprecated` (i.e. the record is present, active, and unexpired).
    pub fn is_valid_proof(env: Env, proof_id_hash: BytesN<32>) -> bool {
        match Self::get_proof(env.clone(), proof_id_hash) {
            Ok(record) => {
                record.status == ProofStatus::Active
                    && env.ledger().timestamp() <= record.expires_at
            }
            Err(_) => false,
        }
    }

    /// Full validity state of a proof: distinguishes pending, active,
    /// revoked, expired, and not-found, where [`Self::is_valid_proof`]
    /// collapses all but "active" to `false`.
    pub fn get_proof_validity(env: Env, proof_id_hash: BytesN<32>) -> ProofValidity {
        match Self::get_proof(env.clone(), proof_id_hash) {
            Ok(record) => Self::compute_validity(env.ledger().timestamp(), &record),
            Err(_) => ProofValidity::NotFound,
        }
    }

    /// Returns the locally-checkable validity summary and revocation timing.
    /// `revoked_ledger == 0` explicitly marks a legacy revoked record whose
    /// stored format predates ledger-sequence revocation metadata.
    pub fn get_proof_validity_details(
        env: Env,
        proof_id_hash: BytesN<32>,
    ) -> Result<ProofValidityDetails, ProofError> {
        let record = Self::get_proof(env.clone(), proof_id_hash)?;
        let revoked = record.status == ProofStatus::Revoked;
        Ok(ProofValidityDetails {
            status: record.status.clone(),
            is_valid: Self::compute_validity(env.ledger().timestamp(), &record)
                == ProofValidity::Active,
            expires_at: record.expires_at,
            revoked,
            revoked_at: if revoked { record.revoked_at } else { 0 },
            revoked_ledger: if revoked { record.revoked_ledger } else { 0 },
        })
    }

    fn compute_validity(now: u64, record: &ProofRecord) -> ProofValidity {
        if record.status == ProofStatus::Revoked {
            return ProofValidity::Revoked;
        }
        if now < record.activates_at {
            return ProofValidity::Pending(record.activates_at);
        }
        if now > record.expires_at {
            return ProofValidity::Expired;
        }
        ProofValidity::Active
    }

    /// Structured proof validity query.
    ///
    /// Returns exactly one [`ProofValidity`] reason, applying this canonical,
    /// deterministic order so that when several invalid conditions hold at
    /// once the earliest one is the reported primary reason:
    ///
    /// 1. `Unknown`          — no record exists for `proof_id_hash`.
    /// 2. `Revoked`          — the record's status is `Revoked`.
    /// 3. `Expired`          — now is strictly after the record's `expires_at`.
    /// 4. `IssuerInactive`   — the issuing address is not currently active.
    /// 5. `SchemaDeprecated` — the record's schema version is not approved.
    /// 6. `Valid`            — none of the above.
    ///
    /// The query is read-only. Its only side effect is the documented TTL
    /// extension performed by [`Self::get_proof`] when the record exists;
    /// absent, revoked, and expired proofs are resolved without any
    /// cross-contract call. If the contract's dependency addresses cannot be
    /// resolved (uninitialized contract), validity cannot be asserted and
    /// `Unknown` is returned.
    pub fn proof_validity(env: Env, proof_id_hash: BytesN<32>) -> ProofValidity {
        let record = match Self::get_proof(env.clone(), proof_id_hash) {
            Ok(record) => record,
            Err(_) => return ProofValidity::Unknown,
        };

        if record.status == ProofStatus::Revoked {
            return ProofValidity::Revoked;
        }

        if env.ledger().timestamp() > record.expires_at {
            return ProofValidity::Expired;
        }

        let issuer_registry = match Self::get_issuer_registry(env.clone()) {
            Ok(address) => address,
            Err(_) => return ProofValidity::Unknown,
        };
        let issuer_client = IssuerRegistryContractClient::new(&env, &issuer_registry);
        if !issuer_client.is_active_address(&record.issuer_address) {
            return ProofValidity::IssuerInactive;
        }

        let protocol_config = match Self::get_protocol_config(env.clone()) {
            Ok(address) => address,
            Err(_) => return ProofValidity::Unknown,
        };
        let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_config);
        if !protocol_client.is_schema_version_approved(&record.schema_version) {
            return ProofValidity::SchemaDeprecated;
        }

        ProofValidity::Valid
    }

    pub fn is_revoked(env: Env, proof_id_hash: BytesN<32>) -> bool {
        match Self::get_proof(env, proof_id_hash) {
            Ok(record) => record.status == ProofStatus::Revoked,
            Err(_) => false,
        }
    }

    pub fn nominate_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env).map_err(|_| ContractError::InvalidState)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_valid_principal(&new_admin)?;
        Self::require_auth(&admin);

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        AdminTransferNominated {
            pending_admin: new_admin.clone(),
            nominated_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn accept_admin(env: Env) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env).map_err(|_| ContractError::InvalidState)?;
        let pending_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(ContractError::NotFound)?;
        Self::require_auth(&pending_admin);

        env.storage()
            .instance()
            .set(&DataKey::Admin, &pending_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);

        AdminTransferAccepted {
            new_admin: pending_admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn cancel_admin_transfer(env: Env) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env).map_err(|_| ContractError::InvalidState)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);

        let pending_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(ContractError::NotFound)?;
        env.storage().instance().remove(&DataKey::PendingAdmin);

        AdminTransferCancelled {
            pending_admin,
            cancelled_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Result {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)
    }

    pub fn get_issuer_registry(env: Env) -> Result {
        env.storage()
            .instance()
            .get(&DataKey::IssuerRegistry)
            .ok_or(ContractError::NotInitialized)
    }

    pub fn get_protocol_config(env: Env) -> Result {
        env.storage()
            .instance()
            .get(&DataKey::ProtocolConfig)
            .ok_or(ContractError::NotInitialized)
    }

    // ── dependency interface handshake ────────────────────────────────────────

    /// Minimum `issuer-registry` interface version this contract accepts. A
    /// bound dependency must report the same major and at least this
    /// minor/patch (see `earnproof_shared::is_interface_compatible`).
    pub fn accepted_issuer_registry_version(_env: Env) -> InterfaceVersion {
        REQUIRED_ISSUER_REGISTRY_VERSION
    }

    /// Minimum `protocol-config` interface version this contract accepts.
    pub fn accepted_protocol_config_version(_env: Env) -> InterfaceVersion {
        REQUIRED_PROTOCOL_CONFIG_VERSION
    }

    /// Live interface version currently reported by the bound issuer registry.
    pub fn bound_issuer_registry_version(env: Env) -> Result {
        let address = Self::get_issuer_registry(env.clone())?;
        Ok(IssuerRegistryContractClient::new(&env, &address).interface_version())
    }

    /// Live interface version currently reported by the bound protocol config.
    pub fn bound_protocol_config_version(env: Env) -> Result {
        let address = Self::get_protocol_config(env.clone())?;
        Ok(ProtocolConfigContractClient::new(&env, &address).interface_version())
    }

    pub fn propose_dependency_replacement(
        env: Env,
        proposal_id: BytesN<32>,
        issuer_registry: Address,
        protocol_config: Address,
        expires_at_ledger: u32,
    ) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::validate_dependency_addresses(&env, &issuer_registry, &protocol_config)?;
        Self::require_compatible_dependencies(&env, &issuer_registry, &protocol_config)?;

        let protocol_config = Self::get_protocol_config(env.clone())?;
        Self::validate_dependency_addresses(&env, &new_issuer_registry, &protocol_config)?;
        let actual =
            IssuerRegistryContractClient::new(&env, &new_issuer_registry).interface_version();
        if !is_interface_compatible(&REQUIRED_ISSUER_REGISTRY_VERSION, &actual) {
            return Err(ContractError::IncompatibleInterfaceVersion);
        }
        if let Some(pending) = env
            .storage()
            .instance()
            .get::<_, PendingDependencyReplacement>(&DataKey::PendingDependencies)
        {
            if now < pending.expires_at_ledger {
                return Err(ContractError::AlreadyExists);
            }
        }
        Self::consume_proposal(&env, &proposal_id)?;
        let pending = PendingDependencyReplacement {
            proposal_id: proposal_id.clone(),
            issuer_registry: issuer_registry.clone(),
            protocol_config: protocol_config.clone(),
            expires_at_ledger,
        };
        env.storage()
            .instance()
            .set(&DataKey::PendingDependencies, &pending);
        Self::extend_instance_ttl(env.clone());
        DependencyReplacementProposed {
            proposal_id,
            issuer_registry,
            protocol_config,
            expires_at_ledger,
            proposed_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_pending_dependencies(env: Env) -> Option<PendingDependencyReplacement> {
        env.storage().instance().get(&DataKey::PendingDependencies)
    }

    pub fn get_last_dependency_replacement(env: Env) -> Option<DependencyReplacementRecord> {
        env.storage()
            .instance()
            .get(&DataKey::LastDependencyReplacement)
    }

    pub fn cancel_dependency_replacement(env: Env) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        let pending: PendingDependencyReplacement = env
            .storage()
            .instance()
            .get(&DataKey::PendingDependencies)
            .ok_or(ContractError::NotFound)?;
        env.storage()
            .instance()
            .remove(&DataKey::PendingDependencies);
        DependencyReplacementCancelled {
            proposal_id: pending.proposal_id,
            cancelled_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn activate_dependency_replacement(
        env: Env,
        proposal_id: BytesN<32>,
    ) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_critical_action_policy_disabled(&env)?;

        let issuer_registry = Self::get_issuer_registry(env.clone())?;
        Self::validate_dependency_addresses(&env, &issuer_registry, &new_protocol_config)?;
        let actual =
            ProtocolConfigContractClient::new(&env, &new_protocol_config).interface_version();
        if !is_interface_compatible(&REQUIRED_PROTOCOL_CONFIG_VERSION, &actual) {
            return Err(ContractError::IncompatibleInterfaceVersion);
        }

        Self::validate_dependency_addresses(
            &env,
            &pending.issuer_registry,
            &pending.protocol_config,
        )?;
        Self::require_compatible_dependencies(
            &env,
            &pending.issuer_registry,
            &pending.protocol_config,
        )?;
        let previous_issuer_registry = Self::get_issuer_registry(env.clone())?;
        let previous_protocol_config = Self::get_protocol_config(env.clone())?;
        let record = DependencyReplacementRecord {
            proposal_id: proposal_id.clone(),
            previous_issuer_registry: previous_issuer_registry.clone(),
            issuer_registry: pending.issuer_registry.clone(),
            previous_protocol_config: previous_protocol_config.clone(),
            protocol_config: pending.protocol_config.clone(),
            activated_at_ledger: env.ledger().sequence(),
        };
        env.storage()
            .instance()
            .set(&DataKey::IssuerRegistry, &pending.issuer_registry);
        env.storage()
            .instance()
            .set(&DataKey::ProtocolConfig, &pending.protocol_config);
        env.storage()
            .instance()
            .set(&DataKey::LastDependencyReplacement, &record);
        env.storage()
            .instance()
            .remove(&DataKey::PendingDependencies);
        Self::extend_instance_ttl(env.clone());
        DependenciesReplaced {
            proposal_id,
            previous_issuer_registry,
            issuer_registry: pending.issuer_registry,
            previous_protocol_config,
            protocol_config: pending.protocol_config,
            activated_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    /// Kept for ABI compatibility; dependency changes must use the paired
    /// proposal and activation flow.
    pub fn set_issuer_registry(
        _env: Env,
        _new_issuer_registry: Address,
    ) -> Result<(), ContractError> {
        Err(ContractError::InvalidState)
    }

    /// Kept for ABI compatibility; dependency changes must use the paired
    /// proposal and activation flow.
    pub fn set_protocol_config(
        _env: Env,
        _new_protocol_config: Address,
    ) -> Result<(), ContractError> {
        Err(ContractError::InvalidState)
    }

    // ── upgrade governance ────────────────────────────────────────────────────

    /// Returns the stored monotonic contract version.  Starts at 1.
    pub fn get_contract_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0)
    }

    /// Admin-only issuer proof capacity. Reductions below active usage are rejected.
    pub fn set_issuer_proof_caps(
        env: Env,
        issuer: Address,
        max_active: u32,
        max_lifetime: u32,
    ) -> Result<(), ProofError> {
        let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        Self::require_auth(&admin);
        let active: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::IssuerActiveProofCount(issuer.clone()))
            .unwrap_or(0);
        let lifetime: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::IssuerLifetimeProofCount(issuer.clone()))
            .unwrap_or(0);
        if max_active < active || max_lifetime < lifetime {
            return Err(ProofError::MalformedInput);
        }
        env.storage().persistent().set(
            &DataKey::IssuerProofCaps(issuer),
            &(max_active, max_lifetime),
        );
        Ok(())
    }
    pub fn get_issuer_proof_usage(env: Env, issuer: Address) -> (u32, u32, u32, u32) {
        let (max_active, max_lifetime) = env
            .storage()
            .persistent()
            .get(&DataKey::IssuerProofCaps(issuer.clone()))
            .unwrap_or((u32::MAX, u32::MAX));
        let active = env
            .storage()
            .persistent()
            .get(&DataKey::IssuerActiveProofCount(issuer.clone()))
            .unwrap_or(0);
        let lifetime = env
            .storage()
            .persistent()
            .get(&DataKey::IssuerLifetimeProofCount(issuer))
            .unwrap_or(0);
        (active, lifetime, max_active, max_lifetime)
    }
    fn consume_issuer_proof_capacity(env: &Env, issuer: &Address) -> Result<(), ProofError> {
        let (active, lifetime, max_active, max_lifetime) =
            Self::get_issuer_proof_usage(env.clone(), issuer.clone());
        if active >= max_active || lifetime >= max_lifetime {
            return Err(ProofError::MalformedInput);
        }
        env.storage().persistent().set(
            &DataKey::IssuerActiveProofCount(issuer.clone()),
            &active.checked_add(1).ok_or(ProofError::MalformedInput)?,
        );
        env.storage().persistent().set(
            &DataKey::IssuerLifetimeProofCount(issuer.clone()),
            &lifetime.checked_add(1).ok_or(ProofError::MalformedInput)?,
        );
        Ok(())
    }

    fn consume_schema_rate_limit(
        env: &Env,
        config: &ProtocolConfigContractClient,
        schema: u32,
    ) -> Result<(), ProofError> {
        let policy = config.get_schema_rate_limit(&schema);
        if policy.window_ledgers == 0 {
            return Err(ProofError::MalformedInput);
        }
        let ledger = env.ledger().sequence();
        let start = ledger - ledger % policy.window_ledgers;
        let key = DataKey::SchemaRateUsage(schema, start);
        let used: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        if used >= policy.max_registrations {
            return Err(ProofError::MalformedInput);
        }
        let next = used.checked_add(1).ok_or(ProofError::MalformedInput)?;
        env.storage().persistent().set(&key, &next);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
        Ok(())
    }

    pub fn get_schema_rate_limit_usage(
        env: Env,
        schema: u32,
    ) -> Result<SchemaRateLimitUsage, ProofError> {
        let config =
            Self::get_protocol_config(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        let policy =
            ProtocolConfigContractClient::new(&env, &config).get_schema_rate_limit(&schema);
        if policy.window_ledgers == 0 {
            return Err(ProofError::MalformedInput);
        }
        let ledger = env.ledger().sequence();
        let start = ledger - ledger % policy.window_ledgers;
        let used = env
            .storage()
            .persistent()
            .get(&DataKey::SchemaRateUsage(schema, start))
            .unwrap_or(0);
        Ok(SchemaRateLimitUsage {
            window_start_ledger: start,
            reset_ledger: start.saturating_add(policy.window_ledgers),
            registrations: used,
            remaining: policy.max_registrations.saturating_sub(used),
        })
    }

    pub fn get_migration_status(env: Env) -> Option {
        env.storage().instance().get(&DataKey::MigrationStatus)
    }

    pub fn begin_migration(env: Env, target_contract_version: u32, total_items: u32) -> Result {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        if target_contract_version <= Self::get_contract_version(env.clone()) || total_items == 0 {
            return Err(ContractError::InvalidInput);
        }
        if let Some(status) = Self::get_migration_status(env.clone()) {
            return if status.target_contract_version == target_contract_version
                && status.total_items == total_items
            {
                Ok(status)
            } else {
                Err(ContractError::InvalidState)
            };
        }
        let status = MigrationStatus {
            status_version: MIGRATION_STATUS_VERSION,
            target_contract_version,
            cursor: 0,
            total_items,
            complete: false,
        };
        env.storage()
            .instance()
            .set(&DataKey::MigrationStatus, &status);
        Self::extend_instance_ttl(env);
        Ok(status)
    }

    pub fn advance_migration(env: Env, expected_cursor: u32, processed_items: u32) -> Result {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        if processed_items == 0 || processed_items > MAX_MIGRATION_BATCH {
            return Err(ContractError::InvalidInput);
        }
        let mut status =
            Self::get_migration_status(env.clone()).ok_or(ContractError::InvalidState)?;
        if expected_cursor < status.cursor {
            return if expected_cursor.saturating_add(processed_items) <= status.cursor {
                Ok(status)
            } else {
                Err(ContractError::InvalidState)
            };
        }
        if expected_cursor != status.cursor || status.complete {
            return Err(ContractError::InvalidState);
        }
        let next = status
            .cursor
            .checked_add(processed_items)
            .ok_or(ContractError::InvalidInput)?;
        if next > status.total_items {
            return Err(ContractError::InvalidInput);
        }
        status.cursor = next;
        status.complete = next == status.total_items;
        env.storage()
            .instance()
            .set(&DataKey::MigrationStatus, &status);
        Self::extend_instance_ttl(env);
        Ok(status)
    }

    pub fn get_config_digest_version() -> u32 {
        earnproof_shared::CONFIG_DIGEST_VERSION
    }

    pub fn get_config_digest(env: Env) -> Result<BytesN<32>, ContractError> {
        let admin = Self::get_admin(env.clone())?;
        let issuer_registry = Self::get_issuer_registry(env.clone())?;
        let protocol_config = Self::get_protocol_config(env.clone())?;
        Ok(earnproof_shared::proof_registry_digest(
            &env,
            &admin,
            &issuer_registry,
            &protocol_config,
            Self::get_contract_version(env.clone()),
        ))
    }

    pub fn get_instance_ttl_status(env: Env) -> TtlStatus {
        earnproof_shared::ttl_status(
            env.ledger().sequence(),
            env.storage().instance().has(&DataKey::Admin),
            env.storage().instance().get(&DataKey::InstanceLiveUntil),
        )
    }

    pub fn get_proof_ttl_status(env: Env, proof_id_hash: BytesN<32>) -> TtlStatus {
        earnproof_shared::ttl_status(
            env.ledger().sequence(),
            env.storage()
                .persistent()
                .has(&DataKey::Proof(proof_id_hash.clone())),
            env.storage()
                .persistent()
                .get(&DataKey::ProofTtl(proof_id_hash)),
        )
    }

    pub fn refresh_instance_ttl(env: Env) -> Result<TtlStatus, ContractError> {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::extend_instance_ttl(env.clone());
        Ok(Self::get_instance_ttl_status(env))
    }

    /// Admin-only: add `wasm_hash` to the upgrade allowlist.
    /// Admin-only: add `wasm_hash` to the upgrade allowlist and record the
    /// `new_version` that must be installed by that WASM.
    pub fn approve_upgrade(
        env: Env,
        wasm_hash: BytesN<32>,
        new_version: u32,
    ) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone()).map_err(|_| ContractError::NotInitialized)?;
        Self::require_auth(&admin);
        if Self::is_scope_paused(&env, PauseScope::Upgrades) {
            panic!("upgrade operations are paused");
        }

        let current = Self::get_contract_version(env.clone());
        if new_version <= current {
            return Err(ContractError::InvalidInput);
        }

        let target_contract = env.current_contract_address();
        let contract_role = Symbol::new(&env, CONTRACT_ROLE);

        let approval_record = UpgradeApprovalRecord {
            new_version,
            target_contract: target_contract.clone(),
            contract_role: contract_role.clone(),
        };

        let key = DataKey::AllowedWasm(wasm_hash.clone());
        env.storage().persistent().set(&key, &approval_record);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);

        let current_ledger = env.ledger().sequence();
        let earliest_execution = current_ledger.saturating_add(UPGRADE_TIMELOCK_LEDGERS);
        let expires_at = current_ledger.saturating_add(UPGRADE_APPROVAL_EXPIRY_LEDGERS);

        if earliest_execution > expires_at {
            return Err(ContractError::InvalidTimingConfig);
        }

        let approval = UpgradeApproval {
            wasm_hash: wasm_hash.clone(),
            created_at: current_ledger,
            earliest_execution,
            expires_at,
            approved_by: admin.clone(),
        };
        env.storage()
            .instance()
            .set(&DataKey::UpgradeApproval, &approval);

        let metadata = UpgradeApprovalMetadata {
            target_hash: wasm_hash.clone(),
            target_version: new_version,
            approver: admin.clone(),
            creation_ledger: current_ledger,
            execution_ledger: None,
            expiry_ledger: expires_at,
            status: ApprovalStatus::Active,
        };
        env.storage().persistent().set(
            &DataKey::UpgradeApprovalMetadata(wasm_hash.clone()),
            &metadata,
        );

        Self::extend_instance_ttl(env.clone());
        UpgradeAllowlisted {
            wasm_hash,
            target_contract,
            contract_role,
            new_contract_version: new_version,
            approved_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    /// Admin-only: remove a hash from the allowlist without applying it.
    pub fn revoke_upgrade(env: Env, wasm_hash: BytesN<32>) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone()).map_err(|_| ContractError::NotInitialized)?;
        Self::require_auth(&admin);
        if Self::is_scope_paused(&env, PauseScope::Upgrades) {
            panic!("upgrade operations are paused");
        }

        let target_contract = env.current_contract_address();
        let contract_role = Symbol::new(&env, CONTRACT_ROLE);

        let key = DataKey::AllowedWasm(wasm_hash.clone());
        env.storage().persistent().remove(&key);

        if let Some(mut metadata) = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalMetadata>(&DataKey::UpgradeApprovalMetadata(
                wasm_hash.clone(),
            ))
        {
            metadata.status = ApprovalStatus::Revoked;
            env.storage().persistent().set(
                &DataKey::UpgradeApprovalMetadata(wasm_hash.clone()),
                &metadata,
            );
        }

        env.storage()
            .instance()
            .remove(&DataKey::AllowedWasm(wasm_hash.clone()));

        env.storage().instance().remove(&DataKey::UpgradeApproval);

        UpgradeRevoked {
            wasm_hash,
            target_contract,
            contract_role,
            revoked_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    /// Returns true when `wasm_hash` is on the allowlist for this contract instance.
    pub fn is_upgrade_allowed(env: Env, wasm_hash: BytesN<32>) -> bool {
        let key = DataKey::AllowedWasm(wasm_hash.clone());
        if let Some(approval) = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalRecord>(&key)
        {
            let current_contract = env.current_contract_address();
            let expected_role = Symbol::new(&env, CONTRACT_ROLE);
            if approval.target_contract != current_contract
                || approval.contract_role != expected_role
            {
                return false;
            }
            return true;
        }
        if let Some(approval) = env
            .storage()
            .instance()
            .get::<_, UpgradeApproval>(&DataKey::UpgradeApproval)
        {
            return approval.wasm_hash == wasm_hash;
        }
        env.storage()
            .instance()
            .has(&DataKey::AllowedWasm(wasm_hash))
    }

    pub fn get_upgrade_approval_metadata(env: Env, target_hash: BytesN<32>) -> ApprovalQuery {
        use earnproof_shared::ApprovalQuery::*;
        use earnproof_shared::ApprovalStatus;

        let metadata = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalMetadata>(&DataKey::UpgradeApprovalMetadata(
                target_hash.clone(),
            ));

        match metadata {
            None => NotFound,
            Some(m) if m.status == ApprovalStatus::Revoked => Revoked(m),
            Some(m) => {
                let current_ledger = env.ledger().sequence();
                if current_ledger > m.expiry_ledger && m.status == ApprovalStatus::Active {
                    Found(UpgradeApprovalMetadata {
                        status: ApprovalStatus::Expired,
                        ..m
                    })
                } else {
                    Found(m)
                }
            }
        }
    }

    /// Admin-only: apply an in-place WASM upgrade with invariant assertions.
    pub fn upgrade_contract(env: Env, wasm_hash: BytesN<32>) {
        let admin = Self::get_admin(env.clone()).expect("contract not initialized");
        Self::require_auth(&admin);
        if Self::is_scope_paused(&env, PauseScope::Upgrades) {
            panic!("upgrade operations are paused");
        }

        let key = DataKey::AllowedWasm(wasm_hash.clone());
        let approval: UpgradeApprovalRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("wasm hash not on allowlist");

        let current_contract = env.current_contract_address();
        let expected_role = Symbol::new(&env, CONTRACT_ROLE);

        if approval.target_contract != current_contract || approval.contract_role != expected_role {
            panic!("upgrade approval does not match target contract identity or role");
        }

        let old_version = Self::get_contract_version(env.clone());
        if approval.new_version <= old_version {
            panic!("upgrade would not advance contract version");
        }

        if let Some(status) = Self::get_migration_status(env.clone()) {
            if !status.complete || status.target_contract_version != approval.new_version {
                panic!("required storage migration is incomplete");
            }
        }

        if let Some(approval_timelock) = env
            .storage()
            .instance()
            .get::<_, UpgradeApproval>(&DataKey::UpgradeApproval)
        {
            if approval_timelock.wasm_hash == wasm_hash {
                let current_ledger = env.ledger().sequence();
                if current_ledger < approval_timelock.earliest_execution {
                    panic!("timelock has not elapsed");
                }
                if current_ledger >= approval_timelock.expires_at {
                    panic!("upgrade approval expired");
                }
            }
        }

        env.storage().persistent().remove(&key);
        env.storage()
            .instance()
            .remove(&DataKey::AllowedWasm(wasm_hash.clone()));
        env.storage().instance().remove(&DataKey::UpgradeApproval);

        if let Some(mut metadata) = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalMetadata>(&DataKey::UpgradeApprovalMetadata(
                wasm_hash.clone(),
            ))
        {
            metadata.status = ApprovalStatus::Executed;
            metadata.execution_ledger = Some(env.ledger().sequence());
            env.storage().persistent().set(
                &DataKey::UpgradeApprovalMetadata(wasm_hash.clone()),
                &metadata,
            );
        }

        #[cfg(not(any(test, feature = "testutils")))]
        env.deployer()
            .update_current_contract_wasm(wasm_hash.clone());

        let old_wasm_hash = env
            .storage()
            .instance()
            .get(&DataKey::CurrentWasmHash)
            .unwrap_or_else(|| BytesN::from_array(&env, &[0u8; 32]));

        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &approval.new_version);
        env.storage()
            .instance()
            .set(&DataKey::CurrentWasmHash, &wasm_hash);

        let current_ledger = env.ledger().sequence();
        let now = env.ledger().timestamp();
        let receipt = UpgradeReceipt {
            wasm_hash: wasm_hash.clone(),
            old_version,
            new_version: approval.new_version,
            upgraded_at: now,
            upgraded_by: admin.clone(),
        };

        env.storage()
            .instance()
            .set(&DataKey::LatestUpgradeReceipt, &receipt);
        env.storage().instance().remove(&DataKey::MigrationStatus);
        Self::extend_instance_ttl(env.clone());

        let history_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeHistoryCount)
            .unwrap_or(0);

        let history_record = UpgradeHistoryRecord {
            old_wasm_hash,
            new_wasm_hash: wasm_hash.clone(),
            old_version,
            new_version: approval.new_version,
            ledger_sequence: current_ledger,
            ledger_timestamp: now,
            upgraded_by: admin.clone(),
        };

        let history_key = DataKey::UpgradeHistory(history_count);
        env.storage()
            .persistent()
            .set(&history_key, &history_record);
        env.storage().persistent().extend_ttl(
            &history_key,
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        env.storage()
            .instance()
            .set(&DataKey::UpgradeHistoryCount, &(history_count + 1));

        ContractUpgraded {
            new_wasm_hash: wasm_hash,
            target_contract: current_contract,
            contract_role: expected_role,
            old_contract_version: old_version,
            new_contract_version: approval.new_version,
            upgraded_by: admin,
        }
        .publish(&env);
    }

    /// Admin-only: revoke an upgrade approval at any point in its lifetime.
    ///
    /// Revocation is allowed:
    /// - Before timelock elapses
    /// - During the valid execution window
    /// - Even after expiry (cleanup)
    ///
    /// # Authorization
    /// Only the admin can revoke.
    pub fn revoke_upgrade_approval(env: Env) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone()).map_err(|_| ContractError::NotInitialized)?;
        Self::require_auth(&admin);

        // Allow revocation even if no approval exists (idempotent)
        env.storage().instance().remove(&DataKey::UpgradeApproval);

        Ok(())
    }

    pub fn get_latest_upgrade_receipt(env: Env) -> Option<UpgradeReceipt> {
        env.storage().instance().get(&DataKey::LatestUpgradeReceipt)
    }

    pub fn get_upgrade_history_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeHistoryCount)
            .unwrap_or(0)
    }

    pub fn get_upgrade_history(env: Env, start: u32, limit: u32) -> Vec<UpgradeHistoryRecord> {
        let total = Self::get_upgrade_history_count(env.clone());
        let mut result = Vec::new(&env);
        if start >= total {
            return result;
        }
        let max_limit = if limit > 50 { 50 } else { limit };
        let end = if start + max_limit > total {
            total
        } else {
            start + max_limit
        };
        for i in start..end {
            if let Some(record) = env
                .storage()
                .persistent()
                .get::<DataKey, UpgradeHistoryRecord>(&DataKey::UpgradeHistory(i))
            {
                result.push_back(record);
            }
        }
        result
    }

    // ── private helpers ───────────────────────────────────────────────────────

    fn validate_dependency_addresses(
        env: &Env,
        issuer_registry: &Address,
        protocol_config: &Address,
    ) -> Result<(), ContractError> {
        if !earnproof_shared::is_valid_principal_address(issuer_registry)
            || !earnproof_shared::is_valid_principal_address(protocol_config)
        {
            return Err(ContractError::InvalidInput);
        }
        let current = env.current_contract_address();
        if issuer_registry == &current
            || protocol_config == &current
            || issuer_registry == protocol_config
        {
            return Err(ContractError::InvalidInput);
        }
        Ok(())
    }

    /// Resolves protocol-config (if configured) and reports whether `scope`
    /// is currently paused. Defaults to not-paused when protocol-config is
    /// not yet set, matching every other pause check in this contract.
    fn is_scope_paused(env: &Env, scope: PauseScope) -> bool {
        if let Ok(protocol_config) = Self::get_protocol_config(env.clone()) {
            let protocol_client = ProtocolConfigContractClient::new(env, &protocol_config);
            protocol_client.is_scope_paused(&scope)
        } else {
            false
        }
    }

    fn assert_operational(env: &Env) {
        if Self::get_migration_status(env.clone()).is_some_and(|status| !status.complete) {
            panic!("storage migration in progress");
        }
    }

    fn require_valid_principal(address: &Address) -> Result<(), ContractError> {
        if !earnproof_shared::is_valid_principal_address(address) {
            return Err(ContractError::InvalidInput);
        }
        Ok(())
    }

    fn require_compatible_dependencies(
        env: &Env,
        issuer_registry: &Address,
        protocol_config: &Address,
    ) -> Result<(), ContractError> {
        let issuer_version =
            IssuerRegistryContractClient::new(env, issuer_registry).interface_version();
        if !earnproof_shared::is_interface_compatible(
            &REQUIRED_ISSUER_REGISTRY_VERSION,
            &issuer_version,
        ) {
            return Err(ContractError::IncompatibleInterfaceVersion);
        }
        let config_version =
            ProtocolConfigContractClient::new(env, protocol_config).interface_version();
        if !earnproof_shared::is_interface_compatible(
            &REQUIRED_PROTOCOL_CONFIG_VERSION,
            &config_version,
        ) {
            return Err(ContractError::IncompatibleInterfaceVersion);
        }
        Ok(())
    }

    fn require_valid_issuer_address(address: &Address) -> Result<(), ProofError> {
        if !earnproof_shared::is_valid_principal_address(address) {
            return Err(ProofError::InvalidAddress);
        }
        Ok(())
    }

    fn set_revoked(env: Env, proof_id_hash: BytesN<32>, by_admin: bool) -> Result<(), ProofError> {
        Self::assert_operational(&env);
        Self::ensure_not_decommissioned(&env)?;
        if let Ok(protocol_config) = Self::get_protocol_config(env.clone()) {
            let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_config);
            if protocol_client.is_scope_paused(&PauseScope::Revocation) {
                return Err(ProofError::ProofNotFound);
            }
        }
        let key = DataKey::Proof(proof_id_hash.clone());
        let mut record: ProofRecord = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ProofError::ProofNotFound)?;

        if by_admin {
            let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
            Self::require_auth(&admin);
        } else {
            Self::require_auth(&record.issuer_address);
        }

        if record.status == ProofStatus::Revoked {
            return Err(ProofError::ProofAlreadyRevoked);
        }

        record.status = ProofStatus::Revoked;
        record.revoked_at = env.ledger().timestamp();
        record.revoked_ledger = env.ledger().sequence();
        let revoked_at = record.revoked_at;
        let revoked_ledger = record.revoked_ledger;
        let active_key = DataKey::IssuerActiveProofCount(record.issuer_address.clone());
        let active: u32 = env.storage().persistent().get(&active_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&active_key, &active.saturating_sub(1));
        env.storage().persistent().set(&key, &record);
        Self::extend_proof_key_ttl(env.clone(), &key);
        let epoch = Self::bump_registry_epoch(&env);
        ProofRevoked {
            proof_id_hash,
            revoked_at,
            revoked_ledger,
            by_admin,
            epoch,
        }
        .publish(&env);
        Ok(())
    }

    fn extend_instance_ttl(env: Env) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
        let live_until = Self::tracked_live_until(&env);
        env.storage()
            .instance()
            .set(&DataKey::InstanceLiveUntil, &live_until);
    }

    fn extend_proof_key_ttl(env: Env, key: &DataKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
        if let DataKey::Proof(proof_id_hash) = key {
            let tracker = DataKey::ProofTtl(proof_id_hash.clone());
            let live_until = Self::tracked_live_until(&env);
            env.storage().persistent().set(&tracker, &live_until);
            env.storage().persistent().extend_ttl(
                &tracker,
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );
        }
    }

    fn extend_payload_key_ttl(env: Env, key: &DataKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
    }

    fn revoke_batch(
        env: Env,
        proof_id_hashes: Vec<BytesN<32>>,
        by_admin: bool,
    ) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;

        let len = proof_id_hashes.len();
        if len == 0 || len > MAX_PROOF_BATCH_SIZE {
            return Err(ProofError::InvalidBatchSize);
        }

        if let Ok(protocol_config) = Self::get_protocol_config(env.clone()) {
            let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_config);
            if protocol_client.is_scope_paused(&PauseScope::Revocation) {
                return Err(ProofError::ProofNotFound);
            }
        }

        // Admin authorization is shared across the whole batch; the issuer
        // path instead authorizes per entry below, against each entry's own
        // recorded issuer.
        let admin = if by_admin {
            let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
            Self::require_auth(&admin);
            Some(admin)
        } else {
            None
        };

        let now = env.ledger().timestamp();

        for proof_id_hash in proof_id_hashes.iter() {
            let key = DataKey::Proof(proof_id_hash.clone());
            let mut record: ProofRecord = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(ProofError::ProofNotFound)?;

            let revoked_by = match &admin {
                Some(admin) => admin.clone(),
                None => {
                    Self::require_auth(&record.issuer_address);
                    record.issuer_address.clone()
                }
            };

            if record.status == ProofStatus::Revoked {
                return Err(ProofError::ProofAlreadyRevoked);
            }

            record.status = ProofStatus::Revoked;
            record.revoked_at = now;
            env.storage().persistent().set(&key, &record);
            // A plain TTL bump, deliberately skipping the per-key TTL
            // tracker write `extend_proof_key_ttl` also performs: a batch of
            // up to `MAX_PROOF_BATCH_SIZE` entries must stay within the
            // per-invocation storage-write budget, and `get_proof_ttl_status`
            // is documented to reflect only calls made outside a batch.
            env.storage().persistent().extend_ttl(
                &key,
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );

            ProofRevokedInBatch {
                proof_id_hash: proof_id_hash.clone(),
                revoked_by,
            }
            .publish(&env);
        }

        Ok(())
    }

    fn read_dispute(env: &Env, proof_id_hash: &BytesN<32>) -> Option<DisputeRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Dispute(proof_id_hash.clone()))
    }

    /// Classifies `actor` relative to a proof: the proof's own issuer, this
    /// contract's admin, or neither.
    fn classify_actor(env: &Env, actor: &Address, issuer_address: &Address) -> DisputeActorClass {
        if actor == issuer_address {
            return DisputeActorClass::Issuer;
        }
        if let Ok(admin) = Self::get_admin(env.clone()) {
            if actor == &admin {
                return DisputeActorClass::Admin;
            }
        }
        DisputeActorClass::ThirdParty
    }

    /// Shared implementation behind [`Self::withdraw_dispute`],
    /// [`Self::resolve_dispute`], and [`Self::reject_dispute`]: every
    /// terminal dispute transition requires an `Open` dispute, is checked
    /// against the `Disputes` pause scope, authorizes the transition's own
    /// actor (the opener for withdrawal, the admin for resolution and
    /// rejection), and emits the matching typed event with that actor's
    /// class and the ledger timestamp.
    fn transition_dispute(
        env: Env,
        proof_id_hash: BytesN<32>,
        transition: DisputeTransition,
    ) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        if let Ok(protocol_config) = Self::get_protocol_config(env.clone()) {
            let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_config);
            if protocol_client.is_scope_paused(&PauseScope::Disputes) {
                return Err(ProofError::ContractPaused);
            }
        }

        let key = DataKey::Dispute(proof_id_hash.clone());
        let mut record: DisputeRecord = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ProofError::DisputeNotFound)?;

        if record.status != DisputeStatus::Open {
            return Err(ProofError::DisputeNotOpen);
        }

        let (actor, new_status) = match transition {
            DisputeTransition::Withdraw => {
                let opener = record.opened_by.clone();
                Self::require_auth(&opener);
                (opener, DisputeStatus::Withdrawn)
            }
            DisputeTransition::Resolve => {
                let admin =
                    Self::get_admin(env.clone()).map_err(|_| ProofError::DisputeNotFound)?;
                Self::require_auth(&admin);
                (admin, DisputeStatus::Resolved)
            }
            DisputeTransition::Reject => {
                let admin =
                    Self::get_admin(env.clone()).map_err(|_| ProofError::DisputeNotFound)?;
                Self::require_auth(&admin);
                (admin, DisputeStatus::Rejected)
            }
        };

        let proof_key = DataKey::Proof(proof_id_hash.clone());
        let proof: ProofRecord = env
            .storage()
            .persistent()
            .get(&proof_key)
            .ok_or(ProofError::ProofNotFound)?;
        let actor_class = Self::classify_actor(&env, &actor, &proof.issuer_address);

        let now = env.ledger().timestamp();
        record.status = new_status;
        record.updated_by = actor.clone();
        record.updated_by_class = actor_class;
        record.updated_at = now;
        env.storage().persistent().set(&key, &record);
        Self::extend_proof_key_ttl(env.clone(), &key);

        match transition {
            DisputeTransition::Withdraw => DisputeWithdrawn {
                proof_id_hash,
                withdrawn_by: actor,
                withdrawn_by_class: actor_class,
                withdrawn_at: now,
            }
            .publish(&env),
            DisputeTransition::Resolve => DisputeResolved {
                proof_id_hash,
                resolved_by: actor,
                resolved_by_class: actor_class,
                resolved_at: now,
            }
            .publish(&env),
            DisputeTransition::Reject => DisputeRejected {
                proof_id_hash,
                rejected_by: actor,
                rejected_by_class: actor_class,
                rejected_at: now,
            }
            .publish(&env),
        }

        Ok(())
    }

    fn bump_registry_epoch(env: &Env) -> u32 {
        let current = Self::get_registry_epoch(env.clone());
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| panic!("registry epoch overflow: reached maximum"));
        env.storage().instance().set(&DataKey::RegistryEpoch, &next);
        next
    }

    fn tracked_live_until(env: &Env) -> u32 {
        env.ledger()
            .sequence()
            .saturating_add(TTL_EXTEND_TO_LEDGERS.min(env.storage().max_ttl()))
    }

    fn require_auth(address: &Address) {
        address.require_auth();
    }

    fn critical_action_policy(env: &Env) -> CriticalActionPolicy {
        env.storage()
            .instance()
            .get(&DataKey::CriticalActionPolicy)
            .unwrap_or(CriticalActionPolicy {
                enabled: false,
                threshold: 0,
                signers: Vec::new(env),
            })
    }

    fn ensure_critical_action_policy_disabled(env: &Env) -> Result<(), ContractError> {
        if Self::critical_action_policy(env).enabled {
            Err(ContractError::ThresholdApprovalRequired)
        } else {
            Ok(())
        }
    }

    fn validate_critical_action_policy(policy: &CriticalActionPolicy) -> Result<(), ContractError> {
        if !policy.enabled {
            return if policy.threshold == 0 && policy.signers.is_empty() {
                Ok(())
            } else {
                Err(ContractError::InvalidApprovalPolicy)
            };
        }
        if policy.signers.is_empty()
            || policy.signers.len() > MAX_CRITICAL_ACTION_SIGNERS
            || policy.threshold == 0
            || policy.threshold > policy.signers.len()
        {
            return Err(ContractError::InvalidApprovalPolicy);
        }
        for (index, signer) in policy.signers.iter().enumerate() {
            if !earnproof_shared::is_valid_principal_address(&signer) {
                return Err(ContractError::InvalidApprovalPolicy);
            }
            for earlier in 0..index {
                if policy.signers.get(earlier as u32).unwrap() == signer {
                    return Err(ContractError::InvalidApprovalPolicy);
                }
            }
        }
        Ok(())
    }

    fn store_critical_action_policy(env: Env, policy: CriticalActionPolicy) {
        env.storage()
            .instance()
            .set(&DataKey::CriticalActionPolicy, &policy);
        Self::extend_instance_ttl(env.clone());
        CriticalActionPolicySet {
            enabled: policy.enabled,
            threshold: policy.threshold,
            signer_count: policy.signers.len(),
        }
        .publish(&env);
    }

    fn ensure_proposal_live(
        env: &Env,
        proposal: &CriticalActionProposal,
    ) -> Result<(), ContractError> {
        if env.ledger().sequence() >= proposal.expires_at {
            return Err(ContractError::ApprovalProposalExpired);
        }
        Ok(())
    }

    fn critical_action_commitment(env: &Env, action: &CriticalAction, nonce: u32) -> BytesN<32> {
        let commitment = (
            Symbol::new(env, "critical_action_v1"),
            env.current_contract_address(),
            action.category(),
            action.clone(),
            nonce,
        );
        env.crypto().sha256(&commitment.to_xdr(env)).to_bytes()
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::{DataKey, ProofRegistryContract, ProofRegistryContractClient};
    use earnproof_shared::{ContractError, ProofError, ProofStatus, TTL_THRESHOLD_LEDGERS};
    use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
    use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
    use soroban_sdk::{
        testutils::{storage::Persistent as _, Address as _, Ledger as _},
        Address, BytesN, Env,
    };

    const ADMIN: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";
    const ISSUER: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";

    fn bytes(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn setup() -> (
        Env,
        ProofRegistryContractClient<'static>,
        ProtocolConfigContractClient<'static>,
        IssuerRegistryContractClient<'static>,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let protocol_config_client = ProtocolConfigContractClient::new(&env, &protocol_config_id);
        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let issuer_registry_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
        let contract_id = env.register(ProofRegistryContract, ());
        let client = ProofRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);
        let issuer_id = bytes(&env, 9);

        protocol_config_client.initialize(&admin);
        protocol_config_client.approve_schema_version(&1);
        protocol_config_client.approve_proof_type(&bytes(&env, 1));
        issuer_registry_client.initialize(&admin);
        issuer_registry_client.register_issuer(
            &issuer_id,
            &issuer,
            &bytes(&env, 8),
            &bytes(&env, 99),
        );
        client.initialize(&admin, &issuer_registry_id, &protocol_config_id);

        (
            env,
            client,
            protocol_config_client,
            issuer_registry_client,
            issuer_registry_id,
        )
    }

    // ── existing tests ────────────────────────────────────────────────────────

    #[test]
    fn registers_and_validates_proof() {
        let (env, client, _protocol_config, _issuer_registry, issuer_registry_id) = setup();
        let proof_id = bytes(&env, 1);
        let commitment = bytes(&env, 2);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof_with_type_identifier(
            &proof_id,
            &commitment,
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        let record = client.get_proof(&proof_id);
        assert_eq!(record.proof_id_hash, proof_id);
        assert_eq!(record.commitment_hash, commitment);
        assert_eq!(record.issuer_address, issuer);
        assert_eq!(record.status, ProofStatus::Active);
        assert_eq!(client.get_issuer_registry(), issuer_registry_id);
        assert!(client.is_valid_proof(&proof_id));
        assert!(!client.is_revoked(&proof_id));
    }

    #[test]
    fn issuer_can_revoke_proof() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.revoke_proof(&proof_id);

        let record = client.get_proof(&proof_id);
        assert_eq!(record.status, ProofStatus::Revoked);
        assert!(client.is_revoked(&proof_id));
        assert!(!client.is_valid_proof(&proof_id));
    }

    #[test]
    fn rejects_expired_proof() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        use earnproof_shared::ProofError;

        let result = client.try_register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &Address::from_str(&env, ISSUER),
            &1,
            &0,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::ProofExpired)));
    }

    #[test]
    fn rejects_duplicate_proof_id() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        use earnproof_shared::ProofError;
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        let result = client.try_register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 3),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::ProofAlreadyRegistered)));
    }

    #[test]
    fn rejects_unapproved_schema_version() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        use earnproof_shared::ProofError;

        let result = client.try_register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &Address::from_str(&env, ISSUER),
            &2,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::UnsupportedSchema)));
    }

    #[test]
    fn rejects_registration_when_protocol_is_paused() {
        let (env, client, protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        use earnproof_shared::ProofError;
        protocol_config.pause(&bytes(&env, 0x91));

        let result = client.try_register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &Address::from_str(&env, ISSUER),
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::ContractPaused)));
    }

    #[test]
    fn rejects_inactive_issuer_address() {
        let (env, client, _protocol_config, issuer_registry, _issuer_registry_id) = setup();
        use earnproof_shared::ProofError;
        let inactive_issuer = Address::from_str(
            &env,
            "GBXHUHG5FGYLPD6RHL2MKWMP572O6KUXCZXDZJXS4T57ZTMAKBN7DWXN",
        );
        issuer_registry.register_issuer(
            &bytes(&env, 10),
            &inactive_issuer,
            &bytes(&env, 11),
            &bytes(&env, 99),
        );
        issuer_registry.suspend_issuer(
            &bytes(&env, 0x95),
            &bytes(&env, 10),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        let result = client.try_register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &inactive_issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::IssuerInactive)));
    }

    #[test]
    fn extends_proof_storage_ttl() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        env.as_contract(&client.address, || {
            assert!(
                env.storage()
                    .persistent()
                    .get_ttl(&DataKey::Proof(proof_id.clone()))
                    > TTL_THRESHOLD_LEDGERS
            );
        });
    }

    // ── upgrade governance tests ──────────────────────────────────────────────

    #[test]
    fn contract_version_initialized_to_one() {
        let (_env, client, ..) = setup();
        assert_eq!(client.get_contract_version(), 1);
    }

    #[test]
    fn approve_and_check_allowlist() {
        let (env, client, ..) = setup();
        let hash = bytes(&env, 0xab);

        assert!(!client.is_upgrade_allowed(&hash));
        client.approve_upgrade(&bytes(&env, 0xa1), &hash, &2);
        assert!(client.is_upgrade_allowed(&hash));
    }

    #[test]
    fn revoke_removes_from_allowlist() {
        let (env, client, ..) = setup();
        let hash = bytes(&env, 0xcd);

        client.approve_upgrade(&bytes(&env, 0xa2), &hash, &2);
        client.revoke_upgrade(&bytes(&env, 0xa3), &hash);
        assert!(!client.is_upgrade_allowed(&hash));
    }

    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn approve_upgrade_rejects_downgrade_version() {
        use earnproof_shared::ContractError;
        let (env, client, ..) = setup();
        let res = client.try_approve_upgrade(&bytes(&env, 1), &1);
        assert_eq!(res, Err(Ok(ContractError::InvalidInput)));
    }

    #[test]
    #[should_panic(expected = "wasm hash not on allowlist")]
    fn upgrade_contract_rejects_non_allowlisted_hash() {
        let (env, client, ..) = setup();
        client.upgrade_contract(&bytes(&env, 0xff));
    }

    #[test]
    #[should_panic]
    fn upgrade_contract_requires_admin_auth() {
        let env = Env::default();
        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let contract_id = env.register(ProofRegistryContract, ());
        let client = ProofRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);
        let issuer_id = bytes(&env, 9);

        env.mock_all_auths();
        let pc_client = ProtocolConfigContractClient::new(&env, &protocol_config_id);
        let ir_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
        pc_client.initialize(&admin);
        pc_client.approve_schema_version(&bytes(&env, 0xa5), &1);
        ir_client.initialize(&admin);
        ir_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8), &bytes(&env, 99));
        client.initialize(&admin, &issuer_registry_id, &protocol_config_id);

        let hash = BytesN::from_array(&env, &[0xde; 32]);
        client.approve_upgrade(&hash, &2);
        env.set_auths(&[]);

        client.upgrade_contract(&hash);
    }

    #[test]
    fn upgrade_advances_version_and_consumes_allowlist() {
        let (env, client, ..) = setup();
        let hash = bytes(&env, 0x42);

        client.approve_upgrade(&bytes(&env, 0xa7), &hash, &2);
        client.upgrade_contract(&hash);

        assert_eq!(client.get_contract_version(), 2);
        assert!(!client.is_upgrade_allowed(&hash));
    }

    #[test]
    #[should_panic(expected = "wasm hash not on allowlist")]
    fn upgrade_hash_cannot_be_replayed() {
        let (env, client, ..) = setup();
        let hash = bytes(&env, 0x42);

        client.approve_upgrade(&bytes(&env, 0xa8), &hash, &2);
        client.upgrade_contract(&hash);
        client.upgrade_contract(&hash);
    }

    /// Persistent proof state must survive an upgrade.
    #[test]
    fn state_preserved_across_upgrade() {
        let (env, client, ..) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(client.is_valid_proof(&proof_id));

        let hash = bytes(&env, 0x77);
        client.approve_upgrade(&bytes(&env, 0xa9), &hash, &2);
        client.upgrade_contract(&hash);

        assert!(client.is_valid_proof(&proof_id));
        assert_eq!(client.get_contract_version(), 2);
    }

    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn cannot_re_approve_old_version_after_upgrade() {
        use earnproof_shared::ContractError;
        let (env, client, ..) = setup();
        let hash_v2 = bytes(&env, 0x01);
        let old_hash = bytes(&env, 0x02);

        client.approve_upgrade(&bytes(&env, 0xaa), &hash_v2, &2);
        client.upgrade_contract(&hash_v2);

        let res = client.try_approve_upgrade(&old_hash, &1);
        assert_eq!(res, Err(Ok(ContractError::InvalidInput)));
    }

    // ── numeric boundary tests ────────────────────────────────────────────────

    /// Table-driven tests for schema version boundaries in proof registration.
    /// Schema versions must be >= MIN_SCHEMA_VERSION (1).
    #[test]
    fn register_proof_schema_version_boundaries() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);

        // Valid: minimum allowed schema version
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(client.is_valid_proof(&bytes(&env, 1)));

        // Valid: typical schema version
        _pc.approve_schema_version(&bytes(&env, 0xac), &99);
        client.register_proof(&bytes(&env, 10), &bytes(&env, 11), &issuer, &99, &2_000);
        assert!(client.is_valid_proof(&bytes(&env, 10)));

        // Valid: large schema version
        _pc.approve_schema_version(&bytes(&env, 0xad), &u32::MAX);
        client.register_proof_with_type_identifier(
            &bytes(&env, 20),
            &bytes(&env, 21),
            &issuer,
            &u32::MAX,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(client.is_valid_proof(&bytes(&env, 20)));
    }

    #[test]
    fn register_proof_schema_version_zero_rejected() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);

        // Schema version 0 must be rejected with a typed error.
        let result = client.try_register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &0,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::InvalidSchemaVersion)));
    }

    /// Table-driven tests for proof expiration boundaries.
    /// Expiration timestamp must be strictly greater than current ledger timestamp.
    #[test]
    fn register_proof_expiration_boundaries() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let current_time = env.ledger().timestamp();

        // Valid: one second in the future (minimum practical offset)
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &(current_time + 1),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(client.is_valid_proof(&bytes(&env, 1)));

        // Valid: reasonable future expiration (1 year in seconds)
        client.register_proof_with_type_identifier(
            &bytes(&env, 10),
            &bytes(&env, 11),
            &issuer,
            &1,
            &(current_time + 365 * 24 * 3600),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(client.is_valid_proof(&bytes(&env, 10)));

        // Valid: the exact schema maximum.
        client.register_proof(
            &bytes(&env, 20),
            &bytes(&env, 21),
            &issuer,
            &1,
            &(current_time + MAX_SCHEMA_VALIDITY_SECONDS),
        );
        assert!(client.is_valid_proof(&bytes(&env, 20)));

        let too_far = client.try_register_proof(
            &bytes(&env, 30),
            &bytes(&env, 31),
            &issuer,
            &1,
            &(current_time + MAX_SCHEMA_VALIDITY_SECONDS + 1),
        );
        assert_eq!(too_far, Err(Ok(ProofError::ProofExpired)));
    }

    #[test]
    fn policy_registration_persists_snapshot_and_rejects_unlisted_type() {
        let (env, client, protocol, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol.deprecate_schema_version(&1);
        let mut allowed_types = soroban_sdk::Vec::new(&env);
        allowed_types.push_back(7);
        protocol.set_schema_policy(&2, &allowed_types, &10);
        protocol.approve_schema_version(&2);

        let now = env.ledger().timestamp();
        client.register_proof_with_policy(
            &bytes(&env, 40),
            &bytes(&env, 41),
            &issuer,
            &2,
            &(now + 10),
            &7,
            &SHA256_COMMITMENT_ALGORITHM_V1,
        );
        assert_eq!(
            client.get_proof_policy_snapshot(&bytes(&env, 40)).unwrap(),
            ProofPolicySnapshot {
                proof_type: 7,
                commitment_algorithm: SHA256_COMMITMENT_ALGORITHM_V1,
                max_validity_seconds: 10,
            }
        );

        protocol.deprecate_schema_version(&2);
        let mut replacement_types = soroban_sdk::Vec::new(&env);
        replacement_types.push_back(9);
        assert!(protocol
            .try_set_schema_policy(&2, &replacement_types, &5)
            .is_err());
        protocol.set_schema_policy(&3, &replacement_types, &5);
        protocol.approve_schema_version(&3);
        assert_eq!(
            client.get_proof_policy_snapshot(&bytes(&env, 40)).unwrap(),
            ProofPolicySnapshot {
                proof_type: 7,
                commitment_algorithm: SHA256_COMMITMENT_ALGORITHM_V1,
                max_validity_seconds: 10,
            }
        );

        let result = client.try_register_proof_with_policy(
            &bytes(&env, 42),
            &bytes(&env, 43),
            &issuer,
            &3,
            &(now + 1),
            &7,
            &SHA256_COMMITMENT_ALGORITHM_V1,
        );
        assert_eq!(result, Err(Ok(ProofError::MalformedInput)));
    }

    #[test]
    fn old_proof_policy_query_has_explicit_legacy_fallback() {
        let (env, client, _protocol, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 70);
        client.register_proof(
            &proof_id,
            &bytes(&env, 71),
            &Address::from_str(&env, ISSUER),
            &1,
            &2_000,
        );
        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .remove(&DataKey::ProofPolicy(proof_id.clone()));
        });
        assert_eq!(
            client.get_proof_policy_snapshot(&proof_id).unwrap(),
            ProofPolicySnapshot {
                proof_type: LEGACY_PROOF_TYPE,
                commitment_algorithm: LEGACY_COMMITMENT_ALGORITHM,
                max_validity_seconds: MAX_SCHEMA_VALIDITY_SECONDS,
            }
        );
    }

    #[test]
    fn retired_commitment_algorithm_cannot_register() {
        let (env, client, protocol, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol.set_commitment_algorithm(&SHA256_COMMITMENT_ALGORITHM_V1, &false);
        let result = client.try_register_proof_with_policy(
            &bytes(&env, 50),
            &bytes(&env, 51),
            &issuer,
            &1,
            &1_000,
            &LEGACY_PROOF_TYPE,
            &SHA256_COMMITMENT_ALGORITHM_V1,
        );
        assert_eq!(result, Err(Ok(ProofError::MalformedInput)));
    }

    #[test]
    fn expiration_near_u64_max_uses_checked_difference() {
        let (env, client, _protocol, _ir, _ir_id) = setup();
        let mut ledger = env.ledger().get();
        ledger.timestamp = u64::MAX - 5;
        env.ledger().set(ledger);
        client.register_proof(
            &bytes(&env, 60),
            &bytes(&env, 61),
            &Address::from_str(&env, ISSUER),
            &1,
            &u64::MAX,
        );
        assert_eq!(
            client.get_proof(&bytes(&env, 60)).unwrap().expires_at,
            u64::MAX
        );
    }

    #[test]
    fn register_proof_expiration_at_current_time_rejected() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let current_time = env.ledger().timestamp();

        // Expiration equal to current time is rejected with a typed error.
        let result = client.try_register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &current_time,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::ProofExpired)));
    }

    #[test]
    fn register_proof_expiration_in_past_rejected() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let current_time = env.ledger().timestamp();

        // Expiration in the past is rejected with a typed error.
        if current_time > 0 {
            let result = client.try_register_proof_with_type_identifier(
                &bytes(&env, 1),
                &bytes(&env, 2),
                &issuer,
                &1,
                &(current_time - 1),
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
            assert_eq!(result, Err(Ok(ProofError::ProofExpired)));
        }
    }

    /// Test storage and event invariants: failed boundary cases
    /// must not modify state or emit events.
    #[test]
    fn failed_register_proof_schema_zero_leaves_state_unchanged() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);

        // Check that no proofs exist initially
        let proof_id = bytes(&env, 99);
        env.as_contract(&client.address, || {
            assert!(
                !env.storage()
                    .persistent()
                    .has(&DataKey::Proof(proof_id.clone())),
                "initial state must not contain the proof"
            );
        });

        // Attempt to register with schema version 0 — should panic
        let register_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof_with_type_identifier(
                &proof_id,
                &bytes(&env, 88),
                &issuer,
                &0,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }));

        // Must have panicked
        assert!(register_result.is_err());

        // State must be unchanged: proof must not exist in storage
        env.as_contract(&client.address, || {
            assert!(
                !env.storage()
                    .persistent()
                    .has(&DataKey::Proof(proof_id.clone())),
                "failed proof registration must not write to storage"
            );
        });
    }

    #[test]
    fn failed_register_proof_expired_leaves_state_unchanged() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let current_time = env.ledger().timestamp();
        let proof_id = bytes(&env, 77);

        // Attempt to register with expired timestamp — should panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof(
                &proof_id,
                &bytes(&env, 66),
                &issuer,
                &1,
                &current_time, // Equal to current time, must be rejected
            );
        }));

        // Must have panicked
        assert!(result.is_err());

        // State must be unchanged: proof must not exist in storage
        env.as_contract(&client.address, || {
            assert!(
                !env.storage().persistent().has(&DataKey::Proof(proof_id)),
                "failed proof registration with expired timestamp must not write to storage"
            );
        });
    }

    /// Verify contract version boundaries in upgrade operations.
    #[test]
    fn contract_version_upgrade_boundaries() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        assert_eq!(client.get_contract_version(), 1);

        // Valid: immediate next version
        client.approve_upgrade(&bytes(&env, 0xb1), &bytes(&env, 1), &2);
        client.upgrade_contract(&bytes(&env, 1));
        assert_eq!(client.get_contract_version(), 2);

        // Valid: large version number
        client.approve_upgrade(&bytes(&env, 0xb2), &bytes(&env, 2), &u32::MAX);
        client.upgrade_contract(&bytes(&env, 2));
        assert_eq!(client.get_contract_version(), u32::MAX);
    }

    // ── adversarial initialization tests ───────────────────────────────────────

    /// Verify that first initialization writes exactly the documented state
    /// with no partial writes or missing fields.
    ///
    /// Required behavior: First call to `initialize` results in:
    /// - Admin address set and readable
    /// - IssuerRegistry address set and readable
    /// - ProtocolConfig address set and readable
    /// - ContractVersion = 1
    #[test]
    fn initialization_writes_exactly_documented_state() {
        let env = Env::default();
        env.mock_all_auths();
        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let contract_id = env.register(ProofRegistryContract, ());
        let client = ProofRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        let pc_client = ProtocolConfigContractClient::new(&env, &protocol_config_id);
        let ir_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
        pc_client.initialize(&admin);
        ir_client.initialize(&admin);

        // Perform initialization
        client.initialize(&admin, &issuer_registry_id, &protocol_config_id);

        // Verify exact state written
        assert_eq!(client.get_admin(), admin, "admin must be set");
        assert_eq!(
            client.get_issuer_registry(),
            issuer_registry_id,
            "issuer registry address must be set"
        );
        assert_eq!(
            client.get_protocol_config(),
            protocol_config_id,
            "protocol config address must be set"
        );
        assert_eq!(
            client.get_contract_version(),
            1,
            "contract version must be exactly 1 after initialization"
        );

        // Verify storage keys are set
        env.as_contract(&contract_id, || {
            let instance = env.storage().instance();
            assert!(
                instance.has(&DataKey::Admin),
                "Admin key must exist in instance storage"
            );
            assert!(
                instance.has(&DataKey::IssuerRegistry),
                "IssuerRegistry key must exist in instance storage"
            );
            assert!(
                instance.has(&DataKey::ProtocolConfig),
                "ProtocolConfig key must exist in instance storage"
            );
            assert!(
                instance.has(&DataKey::ContractVersion),
                "ContractVersion key must exist in instance storage"
            );
        });
    }

    /// Verify that repeated initialization by any address fails without
    /// altering state or emitting events.
    ///
    /// Required behavior for re-initialization guard:
    /// - Second call to `initialize` with any admin (same or different) panics
    /// - Storage is byte-for-byte unchanged
    /// - No additional events are emitted
    #[test]
    fn reinitialization_by_same_admin_fails_atomically() {
        let (env, client, _pc, _ir, ir_id) = setup();
        let admin = Address::from_str(&env, ADMIN);
        let protocol_config_id = env.register(ProtocolConfigContract, ());

        let contract_version_after_first = client.get_contract_version();
        let issuer_registry_after_first = client.get_issuer_registry();

        // Attempt second initialization with same admin
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&admin, &ir_id, &protocol_config_id);
        }));

        // Must have panicked with "already initialized"
        assert!(result.is_err(), "re-initialization must panic");

        // Verify state is byte-for-byte identical
        assert_eq!(
            client.get_admin(),
            admin,
            "admin must not change after failed re-initialization"
        );
        assert_eq!(
            client.get_issuer_registry(),
            issuer_registry_after_first,
            "issuer registry must not change after failed re-initialization"
        );
        assert_eq!(
            client.get_contract_version(),
            contract_version_after_first,
            "contract version must not change after failed re-initialization"
        );
    }

    /// Verify that re-initialization with different dependency addresses
    /// also fails without state changes.
    ///
    /// This tests that the re-initialization guard prevents address swapping.
    #[test]
    fn reinitialization_with_different_dependencies_fails_atomically() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let admin = Address::from_str(&env, ADMIN);

        let issuer_registry_after_first = client.get_issuer_registry();
        let protocol_config_after_first = client.get_protocol_config();

        // Attempt re-initialization with different dependency addresses
        let new_ir = env.register(IssuerRegistryContract, ());
        let new_pc = env.register(ProtocolConfigContract, ());

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&admin, &new_ir, &new_pc);
        }));

        // Must have panicked
        assert!(
            result.is_err(),
            "re-initialization with different deps must panic"
        );

        // Original dependency addresses must be preserved
        assert_eq!(
            client.get_issuer_registry(),
            issuer_registry_after_first,
            "issuer registry must not change when re-initialization attempts different address"
        );
        assert_eq!(
            client.get_protocol_config(),
            protocol_config_after_first,
            "protocol config must not change when re-initialization attempts different address"
        );
    }

    /// Verify that re-initialization by a different admin also fails.
    ///
    /// Tests that the guard does not discriminate based on caller identity.
    #[test]
    fn reinitialization_by_different_admin_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let contract_id = env.register(ProofRegistryContract, ());
        let client = ProofRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let other_admin = Address::from_str(&env, ISSUER);

        let pc_client = ProtocolConfigContractClient::new(&env, &protocol_config_id);
        let ir_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
        pc_client.initialize(&admin);
        ir_client.initialize(&admin);

        // First initialization with original admin
        client.initialize(&admin, &issuer_registry_id, &protocol_config_id);
        let stored_admin = client.get_admin();

        // Attempt re-initialization with different admin
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&other_admin, &issuer_registry_id, &protocol_config_id);
        }));

        // Must have panicked
        assert!(
            result.is_err(),
            "re-initialization by different admin must panic"
        );

        // Original admin must be preserved
        assert_eq!(
            client.get_admin(),
            stored_admin,
            "admin must not change when different address attempts re-initialization"
        );
    }

    /// Verify that invalid dependency addresses are rejected during initialization
    /// and do not write any state.
    ///
    /// Tests initialization with zero/null addresses where contract addresses
    /// are expected. The contract does not validate this at initialization time
    /// (it validates at runtime when dependencies are called), but we should
    /// verify that any panic during initialization leaves state atomic.
    #[test]
    fn reinitialization_guard_is_absolute() {
        let (env, client, _pc, _ir, ir_id) = setup();
        let admin = Address::from_str(&env, ADMIN);
        let pc_id = env.register(ProtocolConfigContract, ());

        // Multiple re-initialization attempts must all fail
        for attempt in 1..=3 {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.initialize(&admin, &ir_id, &pc_id);
            }));

            assert!(
                result.is_err(),
                "re-initialization attempt {} must fail",
                attempt
            );

            // Admin must remain unchanged
            assert_eq!(
                client.get_admin(),
                admin,
                "admin must not change after re-initialization attempt {}",
                attempt
            );
        }
    }

    /// Verify that initialization state is maintained across proof registration
    /// and other operations.
    ///
    /// Tests that the initialization state (admin, dependencies, contract version)
    /// is stable after initialization and before any subsequent operations.
    #[test]
    fn initialization_state_stable_across_operations() {
        let (env, client, _pc, _ir, ir_id) = setup();

        let admin = Address::from_str(&env, ADMIN);
        let protocol_config_id = client.get_protocol_config();

        // State immediately after initialization (from setup())
        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_issuer_registry(), ir_id);
        assert_eq!(client.get_protocol_config(), protocol_config_id);
        assert_eq!(client.get_contract_version(), 1);

        // Perform proof registration
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        // Dependencies must remain unchanged
        assert_eq!(
            client.get_admin(),
            admin,
            "admin must not change after proof registration"
        );
        assert_eq!(
            client.get_issuer_registry(),
            ir_id,
            "issuer registry must not change after proof registration"
        );
        assert_eq!(
            client.get_protocol_config(),
            protocol_config_id,
            "protocol config must not change after proof registration"
        );
        // Contract version must still be 1 (no upgrade yet)
        assert_eq!(
            client.get_contract_version(),
            1,
            "contract version must not change on proof registration"
        );
    }

    /// Summary test: proof-registry initialization spec verification.
    ///
    /// This test serves as executable documentation of what the test matrix
    /// expects from proof-registry initialization:
    /// - Depends on two other contracts (issuer-registry, protocol-config)
    /// - Has re-initialization guard
    /// - Does NOT emit an event during initialization
    /// - Sets: admin, issuer_registry, protocol_config, contract_version=1
    #[test]
    fn proof_registry_initialization_spec_summary() {
        // CONTRACT SPEC: proof-registry
        // - Name: "proof-registry"
        // - Has re-initialization guard: YES (panics "already initialized")
        // - Emits initialization event: NO
        // - Takes dependency addresses: YES
        // - Dependencies: ["issuer-registry", "protocol-config"]
        // - First init writes:
        //   - Admin: passed address (requires auth)
        //   - IssuerRegistry: passed address (no validation at init time)
        //   - ProtocolConfig: passed address (no validation at init time)
        //   - ContractVersion: 1
        // - Re-init guard: DataKey::Admin presence check; panics if set
        // - Re-init allowed by different admin: NO (guard blocks all)
        // - Invalid config cases: Dependency validation happens at runtime (register_proof)
        //   not at initialization time

        let env = Env::default();
        env.mock_all_auths();
        let protocol_config_id = env.register(ProtocolConfigContract, ());
        let issuer_registry_id = env.register(IssuerRegistryContract, ());
        let contract_id = env.register(ProofRegistryContract, ());
        let client = ProofRegistryContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        let pc_client = ProtocolConfigContractClient::new(&env, &protocol_config_id);
        let ir_client = IssuerRegistryContractClient::new(&env, &issuer_registry_id);
        pc_client.initialize(&admin);
        ir_client.initialize(&admin);

        // Verify the spec
        client.initialize(&admin, &issuer_registry_id, &protocol_config_id);
        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_issuer_registry(), issuer_registry_id);
        assert_eq!(client.get_protocol_config(), protocol_config_id);
        assert_eq!(client.get_contract_version(), 1);

        // Re-initialization must fail
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&admin, &issuer_registry_id, &protocol_config_id)
        }))
        .is_err());
    }

    // ── cross-contract initialization and ordering tests ──────────────────────

    /// Verify that the required deployment and initialization ordering is enforced.
    ///
    /// The correct order is:
    /// 1. Deploy protocol-config, initialize with admin
    /// 2. Deploy issuer-registry, initialize with admin
    /// 3. Approve schema version in protocol-config
    /// 4. Register at least one issuer in issuer-registry
    /// 5. Deploy proof-registry, initialize with admin + both dependency addresses
    ///
    /// This test deploys contracts in the correct order and verifies that
    /// the full system initializes successfully end-to-end.
    #[test]
    fn cross_contract_initialization_correct_order_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);
        let issuer_id = bytes(&env, 9);

        // Step 1: Deploy and initialize protocol-config
        let pc_id = env.register(ProtocolConfigContract, ());
        let pc_client = ProtocolConfigContractClient::new(&env, &pc_id);
        pc_client.initialize(&admin);
        assert_eq!(pc_client.get_admin(), admin);
        assert_eq!(pc_client.get_contract_version(), 1);

        // Step 2: Deploy and initialize issuer-registry
        let ir_id = env.register(IssuerRegistryContract, ());
        let ir_client = IssuerRegistryContractClient::new(&env, &ir_id);
        ir_client.initialize(&admin);
        assert_eq!(ir_client.get_admin(), admin);
        assert_eq!(ir_client.get_contract_version(), 1);

        // Step 3: Approve schema version in protocol-config
        pc_client.approve_schema_version(&bytes(&env, 0xc1), &1);
        assert!(pc_client.is_schema_version_approved(&1));

        // Step 4: Register an issuer in issuer-registry
        ir_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8), &bytes(&env, 99));
        assert!(ir_client.is_active_address(&issuer));

        // Step 5: Deploy and initialize proof-registry with both dependencies
        let proof_id = env.register(ProofRegistryContract, ());
        let proof_client = ProofRegistryContractClient::new(&env, &proof_id);
        proof_client.initialize(&admin, &ir_id, &pc_id);
        assert_eq!(proof_client.get_admin(), admin);
        assert_eq!(proof_client.get_issuer_registry(), ir_id);
        assert_eq!(proof_client.get_protocol_config(), pc_id);
        assert_eq!(proof_client.get_contract_version(), 1);

        // Verify the full system is functional: proof registration works
        let proof_id_hash = bytes(&env, 1);
        proof_client.register_proof_with_type_identifier(
            &proof_id_hash,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(proof_client.is_valid_proof(&proof_id_hash));
    }

    /// Verify that proof-registry initialization with uninitialized dependencies
    /// succeeds (no validation at init time), but proof registration fails when
    /// those dependencies are actually needed.
    ///
    /// This tests that initialization stores the dependency addresses without
    /// validating them, and validation happens at runtime (register_proof).
    #[test]
    fn proof_registry_init_with_uninitialized_dependencies_defers_validation() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);

        // Deploy contracts but DON'T initialize the dependencies
        let pc_id = env.register(ProtocolConfigContract, ());
        let ir_id = env.register(IssuerRegistryContract, ());
        let proof_id = env.register(ProofRegistryContract, ());
        let proof_client = ProofRegistryContractClient::new(&env, &proof_id);

        // Proof-registry initialization should succeed even with uninitialized deps
        // (initialization does not validate dependency addresses)
        proof_client.initialize(&admin, &ir_id, &pc_id);
        assert_eq!(proof_client.get_admin(), admin);
        assert_eq!(proof_client.get_issuer_registry(), ir_id);
        assert_eq!(proof_client.get_protocol_config(), pc_id);

        // However, attempting to use the proof registry should fail because
        // the dependencies are not initialized
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            proof_client.register_proof_with_type_identifier(
                &bytes(&env, 1),
                &bytes(&env, 2),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }));

        // Must have panicked (dependencies are not initialized)
        assert!(
            result.is_err(),
            "proof registration must fail with uninitialized dependencies"
        );
    }

    /// Verify that proof-registry with swapped dependency addresses
    /// (issuer-registry address passed where protocol-config address expected)
    /// results in runtime failure when proof operations are attempted.
    ///
    /// This demonstrates that dependency address validation is runtime, not compile-time.
    #[test]
    fn proof_registry_swapped_dependencies_fails_at_runtime() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);
        let issuer_id = bytes(&env, 9);

        // Deploy and initialize all contracts correctly
        let pc_id = env.register(ProtocolConfigContract, ());
        let pc_client = ProtocolConfigContractClient::new(&env, &pc_id);
        pc_client.initialize(&admin);
        pc_client.approve_schema_version(&bytes(&env, 0x90), &1);

        let ir_id = env.register(IssuerRegistryContract, ());
        let ir_client = IssuerRegistryContractClient::new(&env, &ir_id);
        ir_client.initialize(&admin);
        ir_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8), &bytes(&env, 99));

        // Deploy proof-registry
        let proof_id = env.register(ProofRegistryContract, ());
        let proof_client = ProofRegistryContractClient::new(&env, &proof_id);

        // Initialize proof-registry with SWAPPED dependency addresses
        // (pass issuer-registry where protocol-config expected, and vice versa)
        proof_client.initialize(&admin, &pc_id, &ir_id); // Intentionally swapped!
        assert_eq!(proof_client.get_issuer_registry(), pc_id); // Swapped!
        assert_eq!(proof_client.get_protocol_config(), ir_id); // Swapped!

        // Initialization succeeds, but proof registration must fail at runtime
        // because the dependencies are the wrong contracts
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            proof_client.register_proof_with_type_identifier(
                &bytes(&env, 1),
                &bytes(&env, 2),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }));

        // Must have panicked
        assert!(
            result.is_err(),
            "proof registration must fail when dependencies are swapped"
        );
    }

    /// Verify that initialization order matters: proof-registry can be deployed
    /// and initialized BEFORE its dependencies, but operations fail at runtime.
    ///
    /// This demonstrates that Soroban does not enforce deployment-time ordering,
    /// only runtime contract calls enforce dependencies.
    #[test]
    fn proof_registry_initialized_before_dependencies_fails_at_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);

        // Deploy proof-registry FIRST, before dependencies are even deployed
        let proof_id = env.register(ProofRegistryContract, ());
        let proof_client = ProofRegistryContractClient::new(&env, &proof_id);

        // Deploy dependencies (but order is reversed)
        let pc_id = env.register(ProtocolConfigContract, ());
        let ir_id = env.register(IssuerRegistryContract, ());

        // Initialize proof-registry with dependency addresses
        // (they exist as addresses, but aren't initialized yet)
        proof_client.initialize(&admin, &ir_id, &pc_id);

        // Now initialize dependencies
        let pc_client = ProtocolConfigContractClient::new(&env, &pc_id);
        pc_client.initialize(&admin);
        pc_client.approve_schema_version(&bytes(&env, 0x90), &1);

        let ir_client = IssuerRegistryContractClient::new(&env, &ir_id);
        ir_client.initialize(&admin);
        let issuer_id = bytes(&env, 9);
        ir_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8), &bytes(&env, 99));

        // Now proof registration should work because dependencies are initialized
        let proof_id_hash = bytes(&env, 1);
        proof_client.register_proof_with_type_identifier(
            &proof_id_hash,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(proof_client.is_valid_proof(&proof_id_hash));
    }

    /// Verify that attempting to initialize proof-registry without initializing
    /// its dependencies' prerequisites fails at operation time.
    ///
    /// For example: schema version not approved in protocol-config, or issuer
    /// not registered in issuer-registry.
    #[test]
    fn proof_registry_operations_fail_without_dependency_configuration() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);

        // Deploy and initialize all contracts in correct order
        let pc_id = env.register(ProtocolConfigContract, ());
        let pc_client = ProtocolConfigContractClient::new(&env, &pc_id);
        pc_client.initialize(&admin);
        // NOTE: NOT approving schema version 1!

        let ir_id = env.register(IssuerRegistryContract, ());
        let ir_client = IssuerRegistryContractClient::new(&env, &ir_id);
        ir_client.initialize(&admin);
        // NOTE: NOT registering any issuer!

        let proof_id = env.register(ProofRegistryContract, ());
        let proof_client = ProofRegistryContractClient::new(&env, &proof_id);
        proof_client.initialize(&admin, &ir_id, &pc_id);

        // Proof registration should fail because:
        // 1. Schema version 1 is not approved
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            proof_client.register_proof_with_type_identifier(
                &bytes(&env, 1),
                &bytes(&env, 2),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }));
        assert!(
            result.is_err(),
            "proof registration must fail without approved schema version"
        );

        // Now approve schema version but still no issuer registered
        pc_client.approve_schema_version(&bytes(&env, 0x90), &1);

        // Proof registration should fail because issuer is not registered
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            proof_client.register_proof_with_type_identifier(
                &bytes(&env, 2),
                &bytes(&env, 3),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }));
        assert!(
            result.is_err(),
            "proof registration must fail with unregistered issuer"
        );

        // Now register the issuer and everything should work
        let issuer_id = bytes(&env, 9);
        ir_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8), &bytes(&env, 99));

        proof_client.register_proof_with_type_identifier(
            &bytes(&env, 3),
            &bytes(&env, 4),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(proof_client.is_valid_proof(&bytes(&env, 3)));
    }

    /// Verify that all three contracts can be initialized successfully
    /// in their respective dependency order, demonstrating a complete,
    /// valid deployment sequence.
    ///
    /// This is the "happy path" test that confirms the full system
    /// can reach a fully-operational state.
    #[test]
    fn complete_system_initialization_happy_path() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);
        let issuer = Address::from_str(&env, ISSUER);
        let issuer_id = bytes(&env, 9);
        let proof_id_hash = bytes(&env, 1);

        // Initialize protocol-config first (no dependencies)
        let pc_id = env.register(ProtocolConfigContract, ());
        let pc_client = ProtocolConfigContractClient::new(&env, &pc_id);
        pc_client.initialize(&admin);
        pc_client.approve_schema_version(&bytes(&env, 0x90), &1);
        assert!(pc_client.is_schema_version_approved(&1));

        // Initialize issuer-registry second (no dependencies on proof-registry)
        let ir_id = env.register(IssuerRegistryContract, ());
        let ir_client = IssuerRegistryContractClient::new(&env, &ir_id);
        ir_client.initialize(&admin);
        ir_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8), &bytes(&env, 99));
        assert!(ir_client.is_active_address(&issuer));

        // Initialize proof-registry third (depends on both above)
        let proof_id = env.register(ProofRegistryContract, ());
        let proof_client = ProofRegistryContractClient::new(&env, &proof_id);
        proof_client.initialize(&admin, &ir_id, &pc_id);

        // System is now fully operational
        // Verify all initialization invariants
        assert_eq!(pc_client.get_admin(), admin);
        assert_eq!(ir_client.get_admin(), admin);
        assert_eq!(proof_client.get_admin(), admin);

        // Verify all re-initialization guards are in place
        let other_admin = Address::from_str(
            &env,
            "GBXHUHG5FGYLPD6RHL2MKWMP572O6KUXCZXDZJXS4T57ZTMAKBN7DWXN",
        );
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pc_client.initialize(&other_admin)
        }))
        .is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ir_client.initialize(&other_admin)
        }))
        .is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            proof_client.initialize(&other_admin, &ir_id, &pc_id)
        }))
        .is_err());

        // Verify core operations work as expected
        proof_client.register_proof_with_type_identifier(
            &proof_id_hash,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(proof_client.is_valid_proof(&proof_id_hash));

        // Verify state mutations work
        let new_issuer = Address::from_str(
            &env,
            "GBXHUHG5FGYLPD6RHL2MKWMP572O6KUXCZXDZJXS4T57ZTMAKBN7DWXN",
        );
        let new_issuer_id = bytes(&env, 99);
        ir_client.register_issuer(
            &new_issuer_id,
            &new_issuer,
            &bytes(&env, 88),
            &bytes(&env, 99),
        );
        assert!(ir_client.is_active_issuer(&new_issuer_id));

        // Verify admin can still perform admin operations
        pc_client.pause(&bytes(&env, 0x91));
        assert!(pc_client.is_paused());
        pc_client.unpause(&bytes(&env, 0x92));
        assert!(!pc_client.is_paused());
    }

    // ── bounded batch proof registration ─────────────────────────────────────

    use earnproof_shared::ProofRegistrationInput;

    fn batch_input(env: &Env, seed: u8, expires_at: u64) -> ProofRegistrationInput {
        ProofRegistrationInput {
            proof_id_hash: bytes(env, seed),
            commitment_hash: bytes(env, seed.wrapping_add(100)),
            schema_version: 1,
            expires_at,
            proof_type: BytesN::from_array(env, &[1; 32]),
        }
    }

    fn make_batch(
        env: &Env,
        seeds: &[u8],
        expires_at: u64,
    ) -> soroban_sdk::Vec<ProofRegistrationInput> {
        let mut batch = soroban_sdk::Vec::new(env);
        for &seed in seeds {
            batch.push_back(batch_input(env, seed, expires_at));
        }
        batch
    }

    #[test]
    fn register_proofs_batch_registers_every_entry_in_order() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds = [1u8, 2, 3];
        let batch = make_batch(&env, &seeds, 2_000);

        client.register_proofs_batch(&batch, &issuer);

        for &seed in &seeds {
            let proof_id = bytes(&env, seed);
            assert!(client.is_valid_proof(&proof_id));
            let record = client.get_proof(&proof_id);
            assert_eq!(record.issuer_address, issuer);
            assert_eq!(record.status, ProofStatus::Active);
        }
    }

    #[test]
    fn register_proofs_batch_emits_events_in_input_order() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::xdr::{ContractEventBody, ScAddress, ScVal};
        use soroban_sdk::{Map, Symbol, TryFromVal, Val};

        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds = [5u8, 6, 7];
        let batch = make_batch(&env, &seeds, 2_000);

        client.register_proofs_batch(&batch, &issuer);

        let events = env.events().all();
        let proof_ids_in_event_order: std::vec::Vec<BytesN<32>> = events
            .events()
            .iter()
            .filter_map(|event| {
                let contract_id = event.contract_id.clone()?;
                let emitting_contract =
                    Address::try_from_val(&env, &ScVal::Address(ScAddress::Contract(contract_id)))
                        .ok()?;
                if emitting_contract != client.address {
                    return None;
                }
                let ContractEventBody::V0(body) = &event.body;
                let first_topic = body.topics.first()?;
                let first_topic_val = Val::try_from_val(&env, first_topic).ok()?;
                let discriminant = Symbol::try_from_val(&env, &first_topic_val).ok()?;
                if discriminant != Symbol::new(&env, "proof_registered_in_batch") {
                    return None;
                }
                let data_val = Val::try_from_val(&env, &body.data).ok()?;
                let map = Map::<Symbol, Val>::try_from_val(&env, &data_val).ok()?;
                let raw = map.get(Symbol::new(&env, "proof_id_hash"))?;
                BytesN::<32>::try_from_val(&env, &raw).ok()
            })
            .collect();

        assert_eq!(
            proof_ids_in_event_order,
            std::vec![bytes(&env, 5), bytes(&env, 6), bytes(&env, 7)]
        );
    }

    #[test]
    fn register_proofs_batch_rejects_empty_batch() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let empty: soroban_sdk::Vec<ProofRegistrationInput> = soroban_sdk::Vec::new(&env);

        let result = client.try_register_proofs_batch(&empty, &issuer);
        assert_eq!(result, Err(Ok(ProofError::InvalidBatchSize)));
    }

    #[test]
    fn register_proofs_batch_accepts_exact_limit_batch() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds: std::vec::Vec<u8> = (0..MAX_PROOF_BATCH_SIZE as u16).map(|i| i as u8).collect();
        let batch = make_batch(&env, &seeds, 2_000);

        client.register_proofs_batch(&batch, &issuer);

        for &seed in &seeds {
            assert!(client.is_valid_proof(&bytes(&env, seed)));
        }
    }

    #[test]
    fn register_proofs_batch_rejects_over_limit_batch() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds: std::vec::Vec<u8> = (0..(MAX_PROOF_BATCH_SIZE as u16 + 1))
            .map(|i| i as u8)
            .collect();
        let batch = make_batch(&env, &seeds, 2_000);

        let result = client.try_register_proofs_batch(&batch, &issuer);
        assert_eq!(result, Err(Ok(ProofError::InvalidBatchSize)));

        // Nothing from the rejected over-limit batch may have been written.
        for &seed in &seeds {
            assert!(!client.is_valid_proof(&bytes(&env, seed)));
        }
    }

    #[test]
    fn register_proofs_batch_rejects_duplicate_within_batch() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let mut batch = soroban_sdk::Vec::new(&env);
        batch.push_back(batch_input(&env, 1, 2_000));
        batch.push_back(batch_input(&env, 2, 2_000));
        batch.push_back(batch_input(&env, 1, 2_000)); // duplicate of the first

        let result = client.try_register_proofs_batch(&batch, &issuer);
        assert_eq!(result, Err(Ok(ProofError::ProofAlreadyRegistered)));

        // Atomicity: even the entries that would have succeeded (seed 1, 2)
        // must not have been committed.
        assert!(!client.is_valid_proof(&bytes(&env, 1)));
        assert!(!client.is_valid_proof(&bytes(&env, 2)));
    }

    #[test]
    fn register_proofs_batch_rejects_duplicate_against_existing_chain_state() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        let batch = make_batch(&env, &[9, 1, 10], 2_000);
        let result = client.try_register_proofs_batch(&batch, &issuer);
        assert_eq!(result, Err(Ok(ProofError::ProofAlreadyRegistered)));

        // The batch entries preceding the collision must not have been committed.
        assert!(!client.is_valid_proof(&bytes(&env, 9)));
        assert!(!client.is_valid_proof(&bytes(&env, 10)));
    }

    #[test]
    fn register_proofs_batch_rejects_mixed_invalid_entries() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let now = env.ledger().timestamp();

        let mut batch = soroban_sdk::Vec::new(&env);
        batch.push_back(batch_input(&env, 1, 2_000)); // valid
        batch.push_back(ProofRegistrationInput {
            proof_id_hash: bytes(&env, 2),
            commitment_hash: bytes(&env, 102),
            schema_version: 0, // invalid: zero schema version
            expires_at: 2_000,
            proof_type: bytes(&env, 1),
        });
        batch.push_back(batch_input(&env, 3, now)); // invalid: not in the future

        let result = client.try_register_proofs_batch(&batch, &issuer);
        assert_eq!(result, Err(Ok(ProofError::InvalidSchemaVersion)));
        assert!(!client.is_valid_proof(&bytes(&env, 1)));
    }

    #[test]
    fn register_proofs_batch_rejects_unapproved_schema_version() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let mut batch = soroban_sdk::Vec::new(&env);
        batch.push_back(batch_input(&env, 1, 2_000));
        batch.push_back(ProofRegistrationInput {
            proof_id_hash: bytes(&env, 2),
            commitment_hash: bytes(&env, 102),
            schema_version: 42, // never approved
            expires_at: 2_000,
            proof_type: bytes(&env, 1),
        });

        let result = client.try_register_proofs_batch(&batch, &issuer);
        assert_eq!(result, Err(Ok(ProofError::UnsupportedSchema)));
        assert!(!client.is_valid_proof(&bytes(&env, 1)));
    }

    #[test]
    fn register_proofs_batch_rejects_expired_entry() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let now = env.ledger().timestamp();
        let batch = make_batch(&env, &[1], now);

        let result = client.try_register_proofs_batch(&batch, &issuer);
        assert_eq!(result, Err(Ok(ProofError::ProofExpired)));
    }

    #[test]
    fn register_proofs_batch_rejects_when_paused() {
        let (env, client, pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        pc.pause();
        let batch = make_batch(&env, &[1, 2], 2_000);

        let result = client.try_register_proofs_batch(&batch, &issuer);
        assert_eq!(result, Err(Ok(ProofError::ContractPaused)));
    }

    #[test]
    fn register_proofs_batch_rejects_inactive_issuer() {
        let (env, client, _pc, ir, _ir_id) = setup();
        let inactive_issuer = Address::from_str(
            &env,
            "GBXHUHG5FGYLPD6RHL2MKWMP572O6KUXCZXDZJXS4T57ZTMAKBN7DWXN",
        );
        ir.register_issuer(
            &bytes(&env, 10),
            &inactive_issuer,
            &bytes(&env, 11),
            &bytes(&env, 99),
        );
        ir.suspend_issuer(&bytes(&env, 10), &bytes(&env, 1));
        let batch = make_batch(&env, &[1, 2], 2_000);

        let result = client.try_register_proofs_batch(&batch, &inactive_issuer);
        assert_eq!(result, Err(Ok(ProofError::IssuerInactive)));
    }

    #[test]
    fn register_proofs_batch_requires_issuer_auth() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let batch = make_batch(&env, &[1, 2], 2_000);

        env.set_auths(&[]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proofs_batch(&batch, &issuer);
        }));
        assert!(
            result.is_err(),
            "batch registration must require issuer auth"
        );
        assert!(!client.is_valid_proof(&bytes(&env, 1)));
    }

    #[test]
    fn register_proofs_batch_rejects_decommissioned_contract() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let hash = bytes(&env, 0x11);
        let admin = Address::from_str(&env, ADMIN);
        client.nominate_successor(&admin);
        client.activate_successor();
        let _ = hash;

        let batch = make_batch(&env, &[1], 2_000);
        let result = client.try_register_proofs_batch(&batch, &issuer);
        assert_eq!(result, Err(Ok(ProofError::ProofNotFound)));
    }

    // ── delayed proof activation ──────────────────────────────────────────────

    #[test]
    fn register_proof_with_activation_zero_is_immediately_active() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof_with_activation(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &0,
        );

        assert!(client.is_valid_proof(&proof_id));
        assert_eq!(client.get_proof_validity(&proof_id), ProofValidity::Active);
        assert_eq!(client.get_proof(&proof_id).activates_at, 0);
    }

    #[test]
    fn register_proof_with_activation_in_the_past_is_immediate_migration_equivalent() {
        // A record whose activation time is at or before "now" behaves
        // exactly like a proof registered through the plain register_proof
        // path (the pre-existing behavior this feature must not change).
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let now = env.ledger().timestamp();

        client.register_proof_with_activation(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &now,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.register_proof_with_type_identifier(
            &bytes(&env, 3),
            &bytes(&env, 4),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        assert_eq!(
            client.get_proof_validity(&bytes(&env, 1)),
            client.get_proof_validity(&bytes(&env, 3))
        );
        assert!(client.is_valid_proof(&bytes(&env, 1)));
    }

    #[test]
    fn register_proof_with_future_activation_is_pending_until_reached() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let now = env.ledger().timestamp();
        let proof_id = bytes(&env, 1);

        client.register_proof_with_activation(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &(now + 500),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        // Pending proofs cannot verify as valid.
        assert!(!client.is_valid_proof(&proof_id));
        assert_eq!(
            client.get_proof_validity(&proof_id),
            ProofValidity::Pending(now + 500)
        );

        env.ledger().set_timestamp(now + 500);
        assert!(client.is_valid_proof(&proof_id));
        assert_eq!(client.get_proof_validity(&proof_id), ProofValidity::Active);
    }

    #[test]
    fn activation_boundary_is_inclusive() {
        // Exactly at activates_at, the proof is active, not pending.
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let now = env.ledger().timestamp();
        let proof_id = bytes(&env, 1);

        client.register_proof_with_activation(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &(now + 100),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        env.ledger().set_timestamp(now + 99);
        assert_eq!(
            client.get_proof_validity(&proof_id),
            ProofValidity::Pending(now + 100)
        );

        env.ledger().set_timestamp(now + 100);
        assert_eq!(client.get_proof_validity(&proof_id), ProofValidity::Active);
        assert!(client.is_valid_proof(&proof_id));
    }

    #[test]
    fn expired_before_active_is_rejected_at_registration() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);

        // activates_at == expires_at: can never be valid.
        let result = client.try_register_proof_with_activation(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::InvalidActivationTime)));

        // activates_at > expires_at: also can never be valid.
        let result = client.try_register_proof_with_activation(
            &bytes(&env, 3),
            &bytes(&env, 4),
            &issuer,
            &1,
            &2_000,
            &2_001,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::InvalidActivationTime)));

        assert!(!client.is_valid_proof(&bytes(&env, 1)));
        assert!(!client.is_valid_proof(&bytes(&env, 3)));
    }

    #[test]
    fn a_pending_proof_expires_if_never_activated_before_expiry_is_checked() {
        // A pending proof's own expires_at is still checked once activation
        // is reached; expiry after activation is reported as Expired, not
        // Active or Pending.
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let now = env.ledger().timestamp();
        let proof_id = bytes(&env, 1);

        client.register_proof_with_activation(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &(now + 200),
            &(now + 100),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        env.ledger().set_timestamp(now + 300);
        assert_eq!(client.get_proof_validity(&proof_id), ProofValidity::Expired);
        assert!(!client.is_valid_proof(&proof_id));
    }

    #[test]
    fn revoking_a_pending_proof_is_irreversible_and_it_never_activates() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let now = env.ledger().timestamp();
        let proof_id = bytes(&env, 1);

        client.register_proof_with_activation(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &(now + 100),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(
            client.get_proof_validity(&proof_id),
            ProofValidity::Pending(now + 100)
        );

        client.revoke_proof(&proof_id);
        assert_eq!(client.get_proof_validity(&proof_id), ProofValidity::Revoked);

        // Advance past the activation time the proof never reached in an
        // unrevoked state: revocation is terminal regardless.
        env.ledger().set_timestamp(now + 100);
        assert_eq!(client.get_proof_validity(&proof_id), ProofValidity::Revoked);
        assert!(!client.is_valid_proof(&proof_id));

        let rev_info = client.get_revocation_info(&proof_id);
        assert_eq!(rev_info.proof_id_hash, proof_id);
        assert_eq!(rev_info.revoker, issuer);
        assert_eq!(rev_info.revoker_role, earnproof_shared::RevokerRole::Issuer);
        assert_eq!(rev_info.reason_commitment, reason);
    }

    #[test]
    fn admin_revoke_with_reason_stores_revocation_record() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 103);
        let commitment = bytes(&env, 104);
        let issuer = Address::from_str(&env, ISSUER);
        let admin = Address::from_str(&env, ADMIN);
        let reason = bytes(&env, 202);

        client.register_proof(&proof_id, &commitment, &issuer, &1, &5_000);
        client.admin_revoke_proof_with_reason(&proof_id, &reason);

        assert!(client.is_revoked(&proof_id));
        let rev_info = client.get_revocation_info(&proof_id);
        assert_eq!(rev_info.proof_id_hash, proof_id);
        assert_eq!(rev_info.revoker, admin);
        assert_eq!(rev_info.revoker_role, earnproof_shared::RevokerRole::Admin);
        assert_eq!(rev_info.reason_commitment, reason);
    }

    #[test]
    fn duplicate_revocation_is_rejected_and_reason_cannot_be_overwritten() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 105);
        let commitment = bytes(&env, 106);
        let issuer = Address::from_str(&env, ISSUER);
        let reason1 = bytes(&env, 203);
        let reason2 = bytes(&env, 204);

        client.register_proof(&proof_id, &commitment, &issuer, &1, &5_000);
        client.revoke_proof_with_reason(&proof_id, &reason1);

        let err = client.try_revoke_proof_with_reason(&proof_id, &reason2);
        assert_eq!(err, Err(Ok(ProofError::ProofAlreadyRevoked)));

        let rev_info = client.get_revocation_info(&proof_id);
        assert_eq!(rev_info.reason_commitment, reason1);
    }

    // ── Protocol Domain Separation Tests (Issue #145) ──────────────────────

    #[test]
    fn domain_commitment_deterministic_and_separates_domains() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let raw_commitment = bytes(&env, 55);

        let domain_sep = client.get_domain_separator();
        let domain_comm1 = client.compute_domain_commitment(&raw_commitment);
        let domain_comm2 = client.compute_domain_commitment(&raw_commitment);

        assert_eq!(domain_comm1, domain_comm2);
        assert_ne!(raw_commitment, domain_comm1);

        // Cross-domain difference: different raw commitments produce different domain commitments
        let other_raw = bytes(&env, 56);
        let other_comm = client.compute_domain_commitment(&other_raw);
        assert_ne!(domain_comm1, other_comm);

        // Off-chain / shared helper matches contract computation
        let manual_domain_sep = earnproof_shared::compute_domain_separator(
            &env,
            &env.ledger().network_id(),
            &client.address,
        );
        issuer_registry.register_issuer(
            &bytes(&env, 10),
            &inactive_issuer,
            &bytes(&env, 11),
            &bytes(&env, 99),
        );
        issuer_registry.suspend_issuer(&bytes(&env, 10), &bytes(&env, 1));
        let result = client.try_register_proof_with_activation(
            &bytes(&env, 7),
            &bytes(&env, 8),
            &inactive_issuer,
            &1,
            &2_000,
            &0,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::IssuerInactive)));

        // Duplicate proof id.
        client.register_proof_with_activation(
            &bytes(&env, 9),
            &bytes(&env, 91),
            &issuer,
            &1,
            &2_000,
            &0,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        let result = client.try_register_proof_with_activation(
            &bytes(&env, 9),
            &bytes(&env, 92),
            &issuer,
            &1,
            &2_000,
            &0,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(result, Err(Ok(ProofError::ProofAlreadyRegistered)));
    }

    #[test]
    fn get_proof_validity_reports_not_found() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        assert_eq!(
            client.get_proof_validity(&bytes(&env, 99)),
            ProofValidity::NotFound
        );
    }

    // ── bounded batch proof revocation ───────────────────────────────────────

    const ISSUER_TWO: &str = "GDWUSKGGFDI4FRXK5EBTRECZSVQSSWJHHJOGH6JWG3AUMFFMQ435DIAG";

    fn ids(env: &Env, seeds: &[u8]) -> soroban_sdk::Vec<BytesN<32>> {
        let mut v = soroban_sdk::Vec::new(env);
        for &seed in seeds {
            v.push_back(bytes(env, seed));
        }
        v
    }

    #[test]
    fn revoke_proofs_batch_revokes_every_entry_in_order() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds = [1u8, 2, 3];
        for &seed in &seeds {
            client.register_proof_with_type_identifier(
                &bytes(&env, seed),
                &bytes(&env, seed + 100),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }

        client.revoke_proofs_batch(&ids(&env, &seeds));

        for &seed in &seeds {
            assert!(client.is_revoked(&bytes(&env, seed)));
            assert!(!client.is_valid_proof(&bytes(&env, seed)));
        }
    }

    #[test]
    fn revoke_proofs_batch_emits_events_in_input_order() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::xdr::{ContractEventBody, ScAddress, ScVal};
        use soroban_sdk::{Map, Symbol, TryFromVal, Val};

        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds = [5u8, 6, 7];
        for &seed in &seeds {
            client.register_proof_with_type_identifier(
                &bytes(&env, seed),
                &bytes(&env, seed + 100),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }

        client.revoke_proofs_batch(&ids(&env, &seeds));

        let events = env.events().all();
        let proof_ids_in_event_order: std::vec::Vec<BytesN<32>> = events
            .events()
            .iter()
            .filter_map(|event| {
                let contract_id = event.contract_id.clone()?;
                let emitting_contract =
                    Address::try_from_val(&env, &ScVal::Address(ScAddress::Contract(contract_id)))
                        .ok()?;
                if emitting_contract != client.address {
                    return None;
                }
                let ContractEventBody::V0(body) = &event.body;
                let first_topic = body.topics.first()?;
                let first_topic_val = Val::try_from_val(&env, first_topic).ok()?;
                let discriminant = Symbol::try_from_val(&env, &first_topic_val).ok()?;
                if discriminant != Symbol::new(&env, "proof_revoked_in_batch") {
                    return None;
                }
                let data_val = Val::try_from_val(&env, &body.data).ok()?;
                let map = Map::<Symbol, Val>::try_from_val(&env, &data_val).ok()?;
                let raw = map.get(Symbol::new(&env, "proof_id_hash"))?;
                BytesN::<32>::try_from_val(&env, &raw).ok()
            })
            .collect();

        assert_eq!(
            proof_ids_in_event_order,
            std::vec![bytes(&env, 5), bytes(&env, 6), bytes(&env, 7)]
        );
    }

    #[test]
    fn revoke_proofs_batch_rejects_empty_batch() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let empty: soroban_sdk::Vec<BytesN<32>> = soroban_sdk::Vec::new(&env);

        let result = client.try_revoke_proofs_batch(&empty);
        assert_eq!(result, Err(Ok(ProofError::InvalidBatchSize)));
    }

    #[test]
    fn revoke_proofs_batch_accepts_exact_limit_batch() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds: std::vec::Vec<u8> = (0..MAX_PROOF_BATCH_SIZE as u16).map(|i| i as u8).collect();
        for &seed in &seeds {
            client.register_proof_with_type_identifier(
                &bytes(&env, seed),
                &bytes(&env, seed.wrapping_add(100)),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }

        client.revoke_proofs_batch(&ids(&env, &seeds));

        for &seed in &seeds {
            assert!(client.is_revoked(&bytes(&env, seed)));
        }
    }

    #[test]
    fn revoke_proofs_batch_rejects_over_limit_batch() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds: std::vec::Vec<u8> = (0..(MAX_PROOF_BATCH_SIZE as u16 + 1))
            .map(|i| i as u8)
            .collect();
        for &seed in &seeds {
            client.register_proof_with_type_identifier(
                &bytes(&env, seed),
                &bytes(&env, seed.wrapping_add(100)),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }

        let result = client.try_revoke_proofs_batch(&ids(&env, &seeds));
        assert_eq!(result, Err(Ok(ProofError::InvalidBatchSize)));

        for &seed in &seeds {
            assert!(!client.is_revoked(&bytes(&env, seed)));
        }
    }

    #[test]
    fn revoke_proofs_batch_rejects_duplicate_within_batch() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.register_proof_with_type_identifier(
            &bytes(&env, 2),
            &bytes(&env, 102),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        let batch = ids(&env, &[1, 2, 1]); // duplicate of the first
        let result = client.try_revoke_proofs_batch(&batch);
        assert_eq!(result, Err(Ok(ProofError::ProofAlreadyRevoked)));

        // Atomicity: even the entries that would have succeeded must not be committed.
        assert!(!client.is_revoked(&bytes(&env, 1)));
        assert!(!client.is_revoked(&bytes(&env, 2)));
    }

    #[test]
    fn revoke_proofs_batch_rejects_already_revoked_entry() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.register_proof_with_type_identifier(
            &bytes(&env, 2),
            &bytes(&env, 102),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.revoke_proof(&bytes(&env, 1));

        let batch = ids(&env, &[2, 1]);
        let result = client.try_revoke_proofs_batch(&batch);
        assert_eq!(result, Err(Ok(ProofError::ProofAlreadyRevoked)));

        // Entry 2 preceded the failing entry and must not have been committed.
        assert!(!client.is_revoked(&bytes(&env, 2)));
    }

    #[test]
    fn revoke_proofs_batch_rejects_unknown_proof_id() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        let batch = ids(&env, &[1, 99]);
        let result = client.try_revoke_proofs_batch(&batch);
        assert_eq!(result, Err(Ok(ProofError::ProofNotFound)));
        assert!(!client.is_revoked(&bytes(&env, 1)));
    }

    #[test]
    fn revoke_proofs_batch_supports_mixed_ownership_when_every_issuer_authorizes() {
        let (env, client, _pc, ir, _ir_id) = setup();
        let issuer_one = Address::from_str(&env, ISSUER);
        let issuer_two = Address::from_str(&env, ISSUER_TWO);
        ir.register_issuer(
            &bytes(&env, 20),
            &issuer_two,
            &bytes(&env, 21),
            &bytes(&env, 99),
        );

        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer_one,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.register_proof_with_type_identifier(
            &bytes(&env, 2),
            &bytes(&env, 102),
            &issuer_two,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        // env.mock_all_auths() (from setup()) authorizes every address, so a
        // mixed-ownership batch succeeds when both issuers would sign.
        client.revoke_proofs_batch(&ids(&env, &[1, 2]));

        assert!(client.is_revoked(&bytes(&env, 1)));
        assert!(client.is_revoked(&bytes(&env, 2)));
    }

    #[test]
    fn revoke_proofs_batch_rejects_mixed_ownership_without_the_second_issuers_auth() {
        use soroban_sdk::IntoVal;

        let (env, client, _pc, ir, _ir_id) = setup();
        let issuer_one = Address::from_str(&env, ISSUER);
        let issuer_two = Address::from_str(&env, ISSUER_TWO);
        ir.register_issuer(
            &bytes(&env, 20),
            &issuer_two,
            &bytes(&env, 21),
            &bytes(&env, 99),
        );

        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer_one,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.register_proof_with_type_identifier(
            &bytes(&env, 2),
            &bytes(&env, 102),
            &issuer_two,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        // Only issuer_one is authorized for this invocation; issuer_two's
        // entry must not be smuggled through behind it.
        env.set_auths(&[]);
        let auth_entry = soroban_sdk::testutils::MockAuth {
            address: &issuer_one,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &client.address,
                fn_name: "revoke_proofs_batch",
                args: (ids(&env, &[1, 2]),).into_val(&env),
                sub_invokes: &[],
            },
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client
                .mock_auths(&[auth_entry])
                .revoke_proofs_batch(&ids(&env, &[1, 2]));
        }));
        assert!(
            result.is_err(),
            "batch must fail without issuer_two's authorization"
        );
        assert!(!client.is_revoked(&bytes(&env, 1)));
        assert!(!client.is_revoked(&bytes(&env, 2)));
    }

    #[test]
    fn revoke_proofs_batch_rejects_when_revocation_paused() {
        let (env, client, pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        pc.set_scoped_pause(&PauseScope::Revocation, &true);

        let result = client.try_revoke_proofs_batch(&ids(&env, &[1]));
        assert_eq!(result, Err(Ok(ProofError::ProofNotFound)));
        assert!(!client.is_revoked(&bytes(&env, 1)));
    }

    #[test]
    fn revoke_proofs_batch_rejects_decommissioned_contract() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let admin = Address::from_str(&env, ADMIN);
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.nominate_successor(&admin);
        client.activate_successor();

        let result = client.try_revoke_proofs_batch(&ids(&env, &[1]));
        assert_eq!(result, Err(Ok(ProofError::ProofNotFound)));
    }

    #[test]
    fn admin_revoke_proofs_batch_revokes_every_entry() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let seeds = [1u8, 2, 3];
        for &seed in &seeds {
            client.register_proof_with_type_identifier(
                &bytes(&env, seed),
                &bytes(&env, seed + 100),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }

        client.admin_revoke_proofs_batch(&ids(&env, &seeds));

        for &seed in &seeds {
            assert!(client.is_revoked(&bytes(&env, seed)));
        }
    }

    #[test]
    fn admin_revoke_proofs_batch_requires_admin_auth() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        env.set_auths(&[]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.admin_revoke_proofs_batch(&ids(&env, &[1]));
        }));
        assert!(
            result.is_err(),
            "admin batch revocation must require admin auth"
        );
        assert!(!client.is_revoked(&bytes(&env, 1)));
    }

    #[test]
    fn admin_revoke_proofs_batch_can_revoke_proofs_from_multiple_issuers_with_one_auth() {
        let (env, client, _pc, ir, _ir_id) = setup();
        let issuer_one = Address::from_str(&env, ISSUER);
        let issuer_two = Address::from_str(&env, ISSUER_TWO);
        ir.register_issuer(
            &bytes(&env, 20),
            &issuer_two,
            &bytes(&env, 21),
            &bytes(&env, 99),
        );
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 101),
            &issuer_one,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.register_proof_with_type_identifier(
            &bytes(&env, 2),
            &bytes(&env, 102),
            &issuer_two,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        // Only the admin needs to authorize; neither issuer does.
        client.admin_revoke_proofs_batch(&ids(&env, &[1, 2]));

        assert!(client.is_revoked(&bytes(&env, 1)));
        assert!(client.is_revoked(&bytes(&env, 2)));
    }

    // ── proof dispute status lifecycle ──────────────────────────────────────────

    const THIRD_PARTY: &str = "GDWUSKGGFDI4FRXK5EBTRECZSVQSSWJHHJOGH6JWG3AUMFFMQ435DIAG";

    #[test]
    fn open_dispute_by_issuer_is_classified_and_observable() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));

        let dispute = client.get_dispute(&proof_id);
        assert_eq!(dispute.status, DisputeStatus::Open);
        assert_eq!(dispute.opened_by, issuer);
        assert_eq!(dispute.opened_by_class, DisputeActorClass::Issuer);
        assert_eq!(dispute.evidence_commitment, bytes(&env, 30));
    }

    #[test]
    fn open_dispute_by_admin_and_third_party_are_classified_correctly() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let admin = Address::from_str(&env, ADMIN);
        let third_party = Address::from_str(&env, THIRD_PARTY);

        let proof_a = bytes(&env, 1);
        let proof_b = bytes(&env, 2);
        client.register_proof_with_type_identifier(
            &proof_a,
            &bytes(&env, 11),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.register_proof_with_type_identifier(
            &proof_b,
            &bytes(&env, 12),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        client.open_dispute(&proof_a, &admin, &bytes(&env, 30));
        client.open_dispute(&proof_b, &third_party, &bytes(&env, 31));

        assert_eq!(
            client.get_dispute(&proof_a).opened_by_class,
            DisputeActorClass::Admin
        );
        assert_eq!(
            client.get_dispute(&proof_b).opened_by_class,
            DisputeActorClass::ThirdParty
        );
    }

    #[test]
    fn open_dispute_requires_disputant_auth() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let third_party = Address::from_str(&env, THIRD_PARTY);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        env.set_auths(&[]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.open_dispute(&proof_id, &third_party, &bytes(&env, 30));
        }));
        assert!(
            result.is_err(),
            "opening a dispute must require the disputant's auth"
        );
    }

    #[test]
    fn open_dispute_rejects_unknown_proof() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 110);
        let issuer = Address::from_str(&env, ISSUER);

        assert_eq!(
            client.try_open_dispute(&proof_id, &issuer, &bytes(&env, 111)),
            Err(Ok(ProofError::ProofNotFound))
        );
    }

    #[test]
    fn only_one_open_dispute_is_allowed_at_a_time() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let third_party = Address::from_str(&env, THIRD_PARTY);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));
        let result = client.try_open_dispute(&proof_id, &third_party, &bytes(&env, 31));
        assert_eq!(result, Err(Ok(ProofError::DisputeAlreadyOpen)));

        // The rejected attempt must not have overwritten the existing dispute.
        let dispute = client.get_dispute(&proof_id);
        assert_eq!(dispute.opened_by, issuer);
        assert_eq!(dispute.evidence_commitment, bytes(&env, 30));
    }

    #[test]
    fn a_new_dispute_may_be_opened_once_the_prior_one_is_terminal() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let third_party = Address::from_str(&env, THIRD_PARTY);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));
        client.withdraw_dispute(&proof_id);

        // Withdrawn is terminal, not blocking: a new dispute may be opened.
        client.open_dispute(&proof_id, &third_party, &bytes(&env, 31));
        let dispute = client.get_dispute(&proof_id);
        assert_eq!(dispute.status, DisputeStatus::Open);
        assert_eq!(dispute.opened_by, third_party);
        assert_eq!(dispute.evidence_commitment, bytes(&env, 31));
    }

    #[test]
    fn withdraw_dispute_succeeds_for_the_real_opener() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let third_party = Address::from_str(&env, THIRD_PARTY);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.open_dispute(&proof_id, &third_party, &bytes(&env, 30));

        client.withdraw_dispute(&proof_id);
        let dispute = client.get_dispute(&proof_id);
        assert_eq!(dispute.status, DisputeStatus::Withdrawn);
        assert_eq!(dispute.updated_by, third_party);
        assert_eq!(dispute.updated_by_class, DisputeActorClass::ThirdParty);
    }

    #[test]
    fn withdraw_dispute_rejects_an_authorized_address_that_is_not_the_opener() {
        use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
        use soroban_sdk::IntoVal;

        // env.mock_all_auths() would authorize any caller for any
        // require_auth, which would make a test that only checks "someone
        // authorized this" pass even if the contract never checked *whose*
        // authorization it required. Using an explicit MockAuth instead
        // proves the contract requires the opener specifically: another
        // party's own, valid authorization for this exact call is not
        // enough.
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let third_party = Address::from_str(&env, THIRD_PARTY);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));

        let auth_entry = MockAuth {
            address: &third_party,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "withdraw_dispute",
                args: (proof_id.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.mock_auths(&[auth_entry]).withdraw_dispute(&proof_id);
        }));
        assert!(
            result.is_err(),
            "a third party's own authorization must not withdraw the issuer's dispute"
        );
        assert_eq!(client.get_dispute(&proof_id).status, DisputeStatus::Open);
    }

    #[test]
    fn withdraw_dispute_rejects_not_found_and_not_open() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        // No dispute at all.
        let result = client.try_withdraw_dispute(&proof_id);
        assert_eq!(result, Err(Ok(ProofError::DisputeNotFound)));

        // Already withdrawn: not open.
        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));
        client.withdraw_dispute(&proof_id);
        let result = client.try_withdraw_dispute(&proof_id);
        assert_eq!(result, Err(Ok(ProofError::DisputeNotOpen)));
    }

    #[test]
    fn resolve_dispute_is_admin_only_and_terminal() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let admin = Address::from_str(&env, ADMIN);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));

        env.set_auths(&[]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.resolve_dispute(&proof_id);
        }));
        assert!(
            result.is_err(),
            "resolving a dispute must require admin auth"
        );

        env.mock_all_auths();
        client.resolve_dispute(&proof_id);
        let dispute = client.get_dispute(&proof_id);
        assert_eq!(dispute.status, DisputeStatus::Resolved);
        assert_eq!(dispute.updated_by, admin);
        assert_eq!(dispute.updated_by_class, DisputeActorClass::Admin);

        // Terminal: cannot be resolved or rejected again.
        assert_eq!(
            client.try_resolve_dispute(&proof_id),
            Err(Ok(ProofError::DisputeNotOpen))
        );
        assert_eq!(
            client.try_reject_dispute(&proof_id),
            Err(Ok(ProofError::DisputeNotOpen))
        );
    }

    #[test]
    fn reject_dispute_is_admin_only_and_terminal() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let admin = Address::from_str(&env, ADMIN);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));

        env.set_auths(&[]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.reject_dispute(&proof_id);
        }));
        assert!(
            result.is_err(),
            "rejecting a dispute must require admin auth"
        );

        env.mock_all_auths();
        client.reject_dispute(&proof_id);
        let dispute = client.get_dispute(&proof_id);
        assert_eq!(dispute.status, DisputeStatus::Rejected);
        assert_eq!(dispute.updated_by, admin);
        assert_eq!(dispute.updated_by_class, DisputeActorClass::Admin);
    }

    #[test]
    fn resolve_and_reject_reject_not_found_and_not_open() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        assert_eq!(
            client.try_resolve_dispute(&proof_id),
            Err(Ok(ProofError::DisputeNotFound))
        );
        assert_eq!(
            client.try_reject_dispute(&proof_id),
            Err(Ok(ProofError::DisputeNotFound))
        );

        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));
        client.resolve_dispute(&proof_id);

        assert_eq!(
            client.try_resolve_dispute(&proof_id),
            Err(Ok(ProofError::DisputeNotOpen))
        );
        assert_eq!(
            client.try_reject_dispute(&proof_id),
            Err(Ok(ProofError::DisputeNotOpen))
        );
    }

    #[test]
    fn get_dispute_reports_not_found() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let result = client.try_get_dispute(&bytes(&env, 99));
        assert_eq!(result, Err(Ok(ProofError::DisputeNotFound)));
    }

    #[test]
    fn dispute_status_is_independent_of_proof_validity_and_revocation() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        // Disputing an active, valid proof does not change its validity.
        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));
        assert!(client.is_valid_proof(&proof_id));
        assert!(client.verify_domain_commitment(&proof_id, &raw_commitment));

        let wrong_raw = bytes(&env, 112);
        assert!(!client.verify_domain_commitment(&proof_id, &wrong_raw));
    }

    // ── Bounded Batch Proof Validity Queries Tests (Issue #146) ───────────

    #[test]
    fn batch_validity_query_matches_individual_queries_in_order() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.revoke_proof(&proof_id);

        let p1 = bytes(&env, 1);
        let p2 = bytes(&env, 2);
        let p3 = bytes(&env, 3);
        let p_nonexistent = bytes(&env, 99);

        client.register_proof(&p1, &bytes(&env, 11), &issuer, &1, &10_000);
        client.register_proof(&p2, &bytes(&env, 12), &issuer, &1, &10_000);
        client.register_proof(&p3, &bytes(&env, 13), &issuer, &1, &10_000);
        client.revoke_proof(&p2);

        let mut batch_ids = soroban_sdk::Vec::new(&env);
        batch_ids.push_back(p1.clone());
        batch_ids.push_back(p2.clone());
        batch_ids.push_back(p3.clone());
        batch_ids.push_back(p_nonexistent.clone());
        batch_ids.push_back(p1.clone()); // duplicate check

        let batch_results = client.is_valid_proof_batch(&batch_ids);
        assert_eq!(batch_results.len(), 5);
        assert!(batch_results.get(0).unwrap());
        assert!(!batch_results.get(1).unwrap());
        assert!(batch_results.get(2).unwrap());
        assert!(!batch_results.get(3).unwrap());
        assert!(batch_results.get(4).unwrap());
    }

    #[test]
    fn dispute_transitions_reject_when_disputes_are_paused() {
        let (env, client, pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));

        pc.set_scoped_pause(&earnproof_shared::PauseScope::Disputes, &true);

        assert_eq!(
            client.try_open_dispute(&bytes(&env, 2), &issuer, &bytes(&env, 31)),
            Err(Ok(ProofError::ContractPaused))
        );
        assert_eq!(
            client.try_withdraw_dispute(&proof_id),
            Err(Ok(ProofError::ContractPaused))
        );
        assert_eq!(
            client.try_resolve_dispute(&proof_id),
            Err(Ok(ProofError::ContractPaused))
        );
        assert_eq!(
            client.try_reject_dispute(&proof_id),
            Err(Ok(ProofError::ContractPaused))
        );

        // Unrelated global pause does not block disputes: this scope is
        // opted in explicitly, unlike Registration/Updates.
        pc.set_scoped_pause(&earnproof_shared::PauseScope::Disputes, &false);
        pc.pause();
        client.withdraw_dispute(&proof_id);
        assert_eq!(
            client.get_dispute(&proof_id).status,
            DisputeStatus::Withdrawn
        );
    }

    #[test]
    fn dispute_transitions_reject_on_decommissioned_contract() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let admin = Address::from_str(&env, ADMIN);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.open_dispute(&proof_id, &issuer, &bytes(&env, 30));

        client.nominate_successor(&admin);
        client.activate_successor();

        assert_eq!(
            client.try_open_dispute(&bytes(&env, 2), &issuer, &bytes(&env, 31)),
            Err(Ok(ProofError::ProofNotFound))
        );
        assert_eq!(
            client.try_withdraw_dispute(&proof_id),
            Err(Ok(ProofError::ProofNotFound))
        );
        assert_eq!(
            client.try_resolve_dispute(&proof_id),
            Err(Ok(ProofError::ProofNotFound))
        );
        assert_eq!(
            client.try_reject_dispute(&proof_id),
            Err(Ok(ProofError::ProofNotFound))
        );
    }

    #[test]
    fn dispute_events_carry_actor_class_and_ledger_metadata() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::xdr::{ContractEventBody, ScAddress, ScVal};
        use soroban_sdk::{Map, Symbol, TryFromVal, Val};

        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        let result = client.try_is_valid_proof_batch(&oversized_batch);
        assert_eq!(result, Err(Ok(ContractError::InvalidInput)));
    }

    // ── genesis identity (issue #192) ────────────────────────────────────────

    #[test]
    fn genesis_is_recorded_at_initialization() {
        let (env, client, ..) = setup();
        let genesis = client.get_genesis();
        assert_ne!(genesis.genesis_id, BytesN::from_array(&env, &[0u8; 32]));
        assert_eq!(genesis.initialized_at_ledger, env.ledger().sequence());
    }

    #[test]
    fn genesis_is_unchanged_across_an_upgrade() {
        let (env, client, ..) = setup();
        let genesis_before = client.get_genesis();

        let hash = bytes(&env, 0x66);
        client.approve_upgrade(&BytesN::from_array(&env, &[1u8; 32]), &hash, &2);
        client.upgrade_contract(&hash);

        assert_eq!(client.get_genesis(), genesis_before);
    }

    #[test]
    fn get_genesis_fails_before_initialization() {
        let env = Env::default();
        let contract_id = env.register(ProofRegistryContract, ());
        let client = ProofRegistryContractClient::new(&env, &contract_id);
        use earnproof_shared::ContractError;

        let result = client.try_get_genesis();
        assert_eq!(result, Err(Ok(ContractError::NotInitialized)));
    }

    #[test]
    fn genesis_differs_from_a_second_independent_deployment() {
        // Two independently-initialized proof-registry instances must not
        // share a genesis identity, even with the same admin/dependencies.
        let (env, client, protocol_config, issuer_registry, issuer_registry_id) = setup();
        let _ = &protocol_config;
        let _ = &issuer_registry;

        let second_id = env.register(ProofRegistryContract, ());
        let second = ProofRegistryContractClient::new(&env, &second_id);
        second.initialize(
            &Address::from_str(&env, ADMIN),
            &issuer_registry_id,
            &client.get_protocol_config(),
        );

        assert_ne!(
            client.get_genesis().genesis_id,
            second.get_genesis().genesis_id
        );
    }

    // ── registry epoch (issue #187) ───────────────────────────────────────────

    #[test]
    fn registry_epoch_starts_at_zero() {
        let (_env, client, ..) = setup();
        assert_eq!(client.get_registry_epoch(), 0);
    }

    #[test]
    fn register_proof_advances_the_epoch_by_one() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(client.get_registry_epoch(), 1);

        client.register_proof_with_type_identifier(
            &bytes(&env, 3),
            &bytes(&env, 4),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(client.get_registry_epoch(), 2);
    }

    #[test]
    fn revoke_proof_advances_the_epoch() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(client.get_registry_epoch(), 1);

        client.revoke_proof(&proof_id);
        assert_eq!(client.get_registry_epoch(), 2);
    }

    #[test]
    fn admin_revoke_proof_advances_the_epoch() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.admin_revoke_proof(&proof_id);
        assert_eq!(client.get_registry_epoch(), 2);
    }

    #[test]
    fn a_rejected_registration_does_not_advance_the_epoch() {
        let (env, client, protocol_config, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol_config.pause(&BytesN::from_array(&env, &[1u8; 32]));

        let result = client.try_register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert!(result.is_err());
        assert_eq!(client.get_registry_epoch(), 0);
    }

    #[test]
    fn a_double_revocation_does_not_advance_the_epoch_twice() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.revoke_proof(&proof_id);
        let epoch_after_first_revocation = client.get_registry_epoch();

        let result = client.try_revoke_proof(&proof_id);
        assert!(result.is_err());
        assert_eq!(client.get_registry_epoch(), epoch_after_first_revocation);
    }

    #[test]
    fn epoch_overflow_panics_explicitly() {
        let (env, client, ..) = setup();
        env.as_contract(&client.address, || {
            env.storage()
                .instance()
                .set(&DataKey::RegistryEpoch, &u32::MAX);
        });

        let issuer = Address::from_str(&env, ISSUER);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof_with_type_identifier(
                &bytes(&env, 1),
                &bytes(&env, 2),
                &issuer,
                &1,
                &2_000,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }));
        assert!(result.is_err());
    }

    #[test]
    fn registry_epoch_survives_an_upgrade() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        let epoch_before = client.get_registry_epoch();

        let hash = bytes(&env, 0x88);
        client.approve_upgrade(&BytesN::from_array(&env, &[1u8; 32]), &hash, &2);
        client.upgrade_contract(&hash);

        assert_eq!(client.get_registry_epoch(), epoch_before);
    }

    // ── schema-specific payload size limits (issue #190) ─────────────────────

    #[test]
    fn register_proof_with_payload_accepts_a_zero_length_payload() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof_with_type_identifier_and_payload(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &soroban_sdk::Bytes::new(&env),
        );

        let payload = client.get_proof_payload(&proof_id);
        assert_eq!(payload.payload_len, 0);
        assert!(client.is_valid_proof(&proof_id));
    }

    #[test]
    fn register_proof_with_payload_accepts_a_payload_exactly_at_the_limit() {
        let (env, client, protocol_config, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol_config.set_schema_payload_limit(&1, &8);

        let payload_bytes = soroban_sdk::Bytes::from_array(&env, &[0xAB; 8]);
        client.register_proof_with_type_identifier_and_payload(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &payload_bytes,
        );

        let payload = client.get_proof_payload(&bytes(&env, 1));
        assert_eq!(payload.payload_len, 8);
        assert_eq!(
            payload.payload_hash,
            env.crypto().sha256(&payload_bytes).to_bytes()
        );
    }

    #[test]
    fn register_proof_with_payload_rejects_a_payload_one_byte_over_the_limit() {
        let (env, client, protocol_config, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol_config.set_schema_payload_limit(&1, &8);

        let result = client.try_register_proof_with_type_identifier_and_payload(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &soroban_sdk::Bytes::from_array(&env, &[0xAB; 9]),
        );
        assert_eq!(result, Err(Ok(ProofError::MalformedInput)));
    }

    #[test]
    fn register_proof_with_payload_uses_the_default_limit_when_unset() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);

        let ok_payload = soroban_sdk::Bytes::from_array(
            &env,
            &[0u8; earnproof_shared::DEFAULT_SCHEMA_PAYLOAD_LIMIT as usize],
        );
        client.register_proof_with_type_identifier_and_payload(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &ok_payload,
        );
        assert!(client.is_valid_proof(&bytes(&env, 1)));

        let over_payload = soroban_sdk::Bytes::from_array(
            &env,
            &[0u8; (earnproof_shared::DEFAULT_SCHEMA_PAYLOAD_LIMIT + 1) as usize],
        );
        let result = client.try_register_proof_with_type_identifier_and_payload(
            &bytes(&env, 3),
            &bytes(&env, 4),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &over_payload,
        );
        assert_eq!(result, Err(Ok(ProofError::MalformedInput)));
    }

    #[test]
    fn register_proof_with_payload_rejects_a_used_deprecated_schema_the_same_as_registration_without_payload(
    ) {
        let (env, client, protocol_config, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol_config.deprecate_schema_version(&BytesN::from_array(&env, &[1u8; 32]), &1);

        let result = client.try_register_proof_with_type_identifier_and_payload(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &soroban_sdk::Bytes::new(&env),
        );
        assert_eq!(result, Err(Ok(ProofError::UnsupportedSchema)));
    }

    #[test]
    fn a_rejected_payload_registration_writes_nothing() {
        let (env, client, protocol_config, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol_config.set_schema_payload_limit(&1, &4);
        let proof_id = bytes(&env, 1);

        let result = client.try_register_proof_with_type_identifier_and_payload(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            &soroban_sdk::Bytes::from_array(&env, &[0xAB; 5]),
        );
        assert!(result.is_err());
        assert!(!client.is_valid_proof(&proof_id));
        assert_eq!(client.get_registry_epoch(), 0);
        let payload_result = client.try_get_proof_payload(&proof_id);
        assert_eq!(payload_result, Err(Ok(ProofError::ProofNotFound)));
    }

    #[test]
    fn configuration_digest_matches_host_helper_and_version_changes() {
        let (env, client, _pc, _ir, ir_id) = setup();
        let admin = client.get_admin();
        let protocol_config = client.get_protocol_config();
        let initial = client.get_config_digest();
        assert_eq!(
            ProofRegistryContractClient::get_config_digest_version(&client),
            earnproof_shared::CONFIG_DIGEST_VERSION
        );
        assert_eq!(
            initial,
            earnproof_shared::proof_registry_digest(&env, &admin, &ir_id, &protocol_config, 1,)
        );

        let wasm_hash = bytes(&env, 0xd2);
        client.approve_upgrade(&wasm_hash, &2);
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + UPGRADE_TIMELOCK_LEDGERS);
        client.upgrade_contract(&wasm_hash);
        assert_ne!(client.get_config_digest(), initial);
    }

    #[test]
    fn ttl_status_tracks_only_caller_named_proof_entries() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        let proof_id = bytes(&env, 0xe4);
        let unknown_id = bytes(&env, 0xe5);
        let issuer = Address::from_str(&env, ISSUER);

        assert_eq!(
            client.get_instance_ttl_status().health,
            earnproof_shared::TtlHealth::Healthy
        );
        assert_eq!(
            client.get_proof_ttl_status(&unknown_id).health,
            earnproof_shared::TtlHealth::Missing
        );
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 0xe6),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(
            client.get_proof_ttl_status(&proof_id).health,
            earnproof_shared::TtlHealth::Healthy
        );
    }

    // ── structured proof validity reasons (issue 147) ──────────────────────────

    use earnproof_shared::ProofValidity;

    #[test]
    fn proof_validity_reports_valid_for_active_unexpired_proof() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        assert_eq!(client.proof_validity(&proof_id), ProofValidity::Valid);
        // The legacy boolean helper agrees for the happy path.
        assert!(client.is_valid_proof(&proof_id));
    }

    #[test]
    fn proof_validity_reports_unknown_for_missing_proof() {
        let (env, client, ..) = setup();
        assert_eq!(
            client.proof_validity(&bytes(&env, 99)),
            ProofValidity::Unknown
        );
    }

    #[test]
    fn proof_validity_reports_revoked() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.revoke_proof(&proof_id);
        assert_eq!(client.proof_validity(&proof_id), ProofValidity::Revoked);
    }

    #[test]
    fn proof_validity_reports_expired() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        env.ledger().with_mut(|li| li.timestamp = 3_000);
        assert_eq!(client.proof_validity(&proof_id), ProofValidity::Expired);
        // Legacy helper also reports the proof as no longer valid.
        assert!(!client.is_valid_proof(&proof_id));
    }

    #[test]
    fn proof_validity_reports_issuer_inactive() {
        let (env, client, _pc, issuer_registry, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        // Suspend the issuer registered by setup (issuer_id == bytes 9).
        let reason = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
        issuer_registry.suspend_issuer(&bytes(&env, 0x90), &bytes(&env, 9), &reason);
        assert_eq!(
            client.proof_validity(&proof_id),
            ProofValidity::IssuerInactive
        );
    }

    // ── issue 178: dependency interface version handshake ──────────────────────

    use earnproof_shared::InterfaceVersion;
    use soroban_sdk::{contract, contractimpl};

    /// A dependency whose major version differs, so it is rejected.
    #[contract]
    pub struct IncompatibleDependency;

    #[contractimpl]
    impl IncompatibleDependency {
        pub fn is_active_address(_env: Env, _issuer_address: Address) -> bool {
            true
        }
        pub fn is_paused(_env: Env) -> bool {
            false
        }
        pub fn is_schema_version_approved(_env: Env, _version: u32) -> bool {
            true
        }
        pub fn interface_version(_env: Env) -> InterfaceVersion {
            InterfaceVersion::new(99, 0, 0)
        }
    }

    /// A dependency that advances minor/patch within the same major, which the
    /// compatibility rule accepts.
    #[contract]
    pub struct NewerCompatibleDependency;

    #[contractimpl]
    impl NewerCompatibleDependency {
        pub fn is_active_address(_env: Env, _issuer_address: Address) -> bool {
            true
        }
        pub fn is_paused(_env: Env) -> bool {
            false
        }
        pub fn is_schema_version_approved(_env: Env, _version: u32) -> bool {
            true
        }
        pub fn interface_version(_env: Env) -> InterfaceVersion {
            InterfaceVersion::new(1, 5, 3)
        }
    }

    #[test]
    fn exposes_accepted_dependency_versions() {
        let (_env, client, ..) = setup();
        assert_eq!(client.accepted_issuer_registry_version().major, 1);
        assert_eq!(client.accepted_protocol_config_version().major, 1);
    }

    #[test]
    fn reports_bound_dependency_versions() {
        let (_env, client, ..) = setup();
        assert_eq!(
            client.bound_issuer_registry_version(),
            earnproof_shared::ISSUER_REGISTRY_INTERFACE_VERSION
        );
        assert_eq!(
            client.bound_protocol_config_version(),
            earnproof_shared::PROTOCOL_CONFIG_INTERFACE_VERSION
        );
    }

    #[test]
    fn proof_validity_reports_schema_deprecated() {
        let (env, client, protocol_config, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
        protocol_config.deprecate_schema_version(&bytes(&env, 0x91), &1);
        assert_eq!(
            client.proof_validity(&proof_id),
            ProofValidity::SchemaDeprecated
        );
    }

    /// When several invalid conditions hold at once, the canonical order makes
    /// the earliest one the primary reason: revoked precedes expired.
    #[test]
    fn proof_validity_precedence_revoked_before_expired() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        client.revoke_proof(&proof_id);
        env.ledger().with_mut(|li| li.timestamp = 3_000);
        assert_eq!(client.proof_validity(&proof_id), ProofValidity::Revoked);
    }

    /// Precedence: expired precedes issuer-inactive and schema-deprecated.
    #[test]
    fn proof_validity_precedence_expired_before_issuer_and_schema() {
        let (env, client, protocol_config, issuer_registry, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
        let reason = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
        issuer_registry.suspend_issuer(&bytes(&env, 0x92), &bytes(&env, 9), &reason);
        protocol_config.deprecate_schema_version(&bytes(&env, 0x93), &1);
        env.ledger().with_mut(|li| li.timestamp = 3_000);
        assert_eq!(client.proof_validity(&proof_id), ProofValidity::Expired);
    }

    // ── proof creation ledger + timestamp metadata (issue 184) ─────────────────

    #[test]
    fn register_proof_records_creation_ledger_and_timestamp() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        env.ledger().with_mut(|li| {
            li.sequence_number = 4_321;
            li.timestamp = 1_500;
        });
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &5_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        let record = client.get_proof(&proof_id);
        assert_eq!(record.created_ledger, 4_321);
        assert_eq!(record.created_at, 1_500);
    }

    #[test]
    fn register_proof_emits_one_proof_registered_event() {
        use soroban_sdk::testutils::Events;
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof_with_type_identifier(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );
        // register_proof publishes exactly one contract event carrying timing.
        assert_eq!(env.events().all().events().len(), 1);
    }

    #[test]
    fn failed_register_proof_emits_no_event() {
        use soroban_sdk::testutils::Events;
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        // Expired at registration time: rejected, so no event and no record.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof_with_type_identifier(
                &bytes(&env, 1),
                &bytes(&env, 2),
                &issuer,
                &1,
                &0,
                &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
            );
        }));
        assert!(result.is_err());
        assert_eq!(env.events().all().events().len(), 0);
    }

    #[test]
    fn initialization_rejects_incompatible_dependency_before_state_mutation() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);
        let good_config = env.register(ProtocolConfigContract, ());
        let bad_registry = env.register(IncompatibleDependency, ());

        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
        let result = proofs.try_initialize(&admin, &bad_registry, &good_config);
        assert_eq!(
            result,
            Err(Ok(
                earnproof_shared::ContractError::IncompatibleInterfaceVersion
            ))
        );
        // No admin was written: the contract remains uninitialized.
        assert!(proofs.try_get_admin().is_err());
    }

    #[test]
    fn governed_replacement_accepts_a_newer_compatible_dependency() {
        let (env, client, ..) = setup();
        let newer = env.register(NewerCompatibleDependency, ());
        let proposal_id = bytes(&env, 0x95);
        let expiry = env.ledger().sequence() + 100;
        let original_registry = client.get_issuer_registry();
        let protocol_config = client.get_protocol_config();
        client.propose_dependency_replacement(&proposal_id, &newer, &protocol_config, &expiry);
        let pending = client.get_pending_dependencies().unwrap();
        assert_eq!(pending.issuer_registry, newer);
        assert_eq!(pending.protocol_config, protocol_config);
        client.activate_dependency_replacement(&proposal_id);
        assert_eq!(client.get_issuer_registry(), newer);
        assert_eq!(client.bound_issuer_registry_version().minor, 5);
        let replacement = client.get_last_dependency_replacement().unwrap();
        assert_eq!(replacement.previous_issuer_registry, original_registry);
        assert_eq!(replacement.issuer_registry, newer);
    }

    #[test]
    fn governed_replacement_rejects_an_incompatible_dependency() {
        let (env, client, ..) = setup();
        let original = client.get_issuer_registry();
        let bad = env.register(IncompatibleDependency, ());

        let protocol_config = client.get_protocol_config();
        let result = client.try_propose_dependency_replacement(
            &bytes(&env, 0x96),
            &bad,
            &protocol_config,
            &(env.ledger().sequence() + 100),
        );
        assert_eq!(
            result,
            Err(Ok(
                earnproof_shared::ContractError::IncompatibleInterfaceVersion
            ))
        );
        // The binding is unchanged: the rejected replacement mutated nothing.
        assert_eq!(client.get_issuer_registry(), original);
    }

    #[test]
    fn governed_replacement_is_not_bypassed_by_paused_state() {
        let (env, client, protocol_config, ..) = setup();
        protocol_config.pause(&bytes(&env, 0x94));
        let bad = env.register(IncompatibleDependency, ());

        // Even while paused, the interface check still runs and rejects.
        let result = client.try_propose_dependency_replacement(
            &bytes(&env, 0x97),
            &bad,
            &client.get_protocol_config(),
            &(env.ledger().sequence() + 100),
        );
        assert_eq!(
            result,
            Err(Ok(
                earnproof_shared::ContractError::IncompatibleInterfaceVersion
            ))
        );
    }
}
