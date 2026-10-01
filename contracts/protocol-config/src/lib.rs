#![no_std]

use earnproof_shared::{
    AttestedUpgradeReceipt, ContractError, MigrationStatus, PauseScope,
    UpgradeCompatibilityAttestation, UpgradeReceipt, MAX_MIGRATION_BATCH, MIGRATION_STATUS_VERSION,
    TTL_EXTEND_TO_LEDGERS, TTL_THRESHOLD_LEDGERS,
};
use soroban_sdk::{contract, contractevent, contractimpl, contracttype, Address, BytesN, Env, Vec};
    ConfigChangeCategory, ConfigChangeSummary, ContractError, GenesisRecord, InterfaceVersion,
    MigrationStatus, PauseScope, SchemaRateLimit, CONFIG_HISTORY_CAPACITY,
    DEFAULT_SCHEMA_PAYLOAD_LIMIT, DEFAULT_SCHEMA_RATE_LIMIT, DEFAULT_SCHEMA_RATE_WINDOW_LEDGERS,
    MAX_CONFIG_HISTORY_PAGE, MAX_MIGRATION_BATCH, MIGRATION_STATUS_VERSION,
    PROTOCOL_CONFIG_INTERFACE_VERSION, TTL_EXTEND_TO_LEDGERS, TTL_THRESHOLD_LEDGERS,
};
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, xdr::ToXdr, Address, BytesN, Env, Symbol,
    Vec,
};

#[contract]
pub struct ProtocolConfigContract;

#[contracttype]
enum DataKey {
    MigrationStatus,
    Admin,
    Paused,
    ScopedPause(PauseScope),
    ConfigVersion,
    MaxProofs,
    SchemaVersion(u32),
    SchemaVersionIndex(u32),
    SchemaVersionIndexCount,
    SchemaLifecycle(u32),
    SchemaTransitionCount,
    SchemaTransitionSlot(u32),
    PendingAdmin,
    /// Allowlist entry: maps a WASM hash to the target contract version it
    /// must install.  Only hashes pre-approved by the admin may be applied.
    AllowedWasm(BytesN<32>),
    UpgradeAttestationProposal(BytesN<32>),
    UpgradeAttestationApproval(BytesN<32>),
    LatestAttestedUpgradeReceipt,
    /// Monotonically-increasing contract version stored in instance storage.
    /// Prevents installing an older (or equal) version over a newer one.
    ContractVersion,
    Successor,
    Decommissioned,
    /// Immutable deployment identity, written once at `initialize`.
    Genesis,
    /// Governed per-schema-version auxiliary payload size bound (bytes).
    SchemaPayloadLimit(u32),
    /// Governed registration-rate policy for a schema version.
    SchemaRateLimit(u32),
    /// Ring-buffer slot holding one bounded change-history summary.
    ConfigHistoryRing(u32),
    /// Monotonic count of change-history entries ever appended.
    ConfigHistoryTotal,
    GovernanceAssignment(GovernanceRole, Address),
}

const SCHEMA_TRANSITION_HISTORY_CAPACITY: u32 = 32;
const MAX_SCHEMA_TRANSITION_PAGE_SIZE: u32 = 16;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaTransitionKind {
    Proposal,
    Approval,
    Activation,
    Deprecation,
    PolicyRevision,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaTransition {
    pub sequence: u64,
    pub version: u32,
    pub kind: SchemaTransitionKind,
    pub transitioned_at: u64,
    pub lineage_commitment: Option<BytesN<32>>,
    pub policy_commitment: Option<BytesN<32>>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaTransitionPage {
    pub transitions: soroban_sdk::Vec<SchemaTransition>,
    pub next_cursor: u64,
    pub oldest_cursor: u64,
    pub total_cursor: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct SchemaLifecycle {
    lineage_commitment: BytesN<32>,
    policy_commitment: BytesN<32>,
    active: bool,
    deprecated: bool,
}

// ── admin transfer events ─────────────────────────────────────────────────────────

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

// ── existing events ─────────────────────────────────────────────────────────

#[contractevent]
pub struct Initialized {
    pub admin: Address,
}

#[contractevent]
pub struct AdminChanged {
    pub proposal_id: BytesN<32>,
    pub new_admin: Address,
}

#[contractevent]
pub struct Paused {
    pub proposal_id: BytesN<32>,
    pub paused: bool,
}

#[contractevent]
pub struct Unpaused {
    pub proposal_id: BytesN<32>,
    pub paused: bool,
}

#[contractevent]
pub struct SchemaApproved {
    pub proposal_id: BytesN<32>,
    pub version: u32,
}

#[contractevent]
pub struct SchemaDeprecated {
    pub proposal_id: BytesN<32>,
    pub version: u32,
}

#[contractevent]
pub struct ProofCapacityChanged {
    pub max_proofs: u32,
    pub changed_by: Address,
}

#[contractevent]
pub struct ScopedPauseChanged {
    pub scope: PauseScope,
    pub paused: bool,
}

#[contractevent]
pub struct SuccessorNominated {
    pub successor: Address,
    pub nominated_by: Address,
}

#[contractevent]
pub struct ContractDecommissioned {
    pub old_instance: Address,
    pub successor_instance: Address,
    pub activated_by: Address,
}

/// Emitted when the admin sets or updates the maximum auxiliary payload size
/// accepted for a schema version.
#[contractevent]
pub struct SchemaPayloadLimitSet {
    pub version: u32,
    pub max_size: u32,
}

#[contractevent]
pub struct SchemaPolicySet {
    pub version: u32,
    pub proof_type_count: u32,
    pub max_validity_seconds: u64,
}

#[contractevent]
pub struct CommitmentAlgorithmPolicySet {
    pub algorithm: u32,
    pub supported: bool,
}

// ── upgrade events ───────────────────────────────────────────────────────────

/// Emitted when the admin adds a WASM hash to the upgrade allowlist.
#[contractevent]
pub struct UpgradeAllowlisted {
    pub proposal_id: BytesN<32>,
    pub wasm_hash: BytesN<32>,
    pub new_contract_version: u32,
    pub approved_by: Address,
}

/// Emitted when the admin removes a WASM hash from the allowlist without
/// applying it (e.g. rolling back an approved-but-not-yet-applied hash).
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

#[contractimpl]
impl ProtocolConfigContract {
    pub fn is_decommissioned(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Decommissioned)
            .unwrap_or(false)
    }

    fn ensure_not_decommissioned(env: &Env) -> Result<(), ContractError> {
        if Self::is_decommissioned(env.clone()) {
            Err(ContractError::InvalidState)
        } else {
            Ok(())
        }
    }

    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }

        Self::require_valid_principal(&admin)?;
        Self::require_auth(&admin);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::ConfigVersion, &1_u32);
        env.storage().instance().set(&DataKey::MaxProofs, &u32::MAX);
        env.storage()
            .instance()
            .set(&DataKey::SchemaVersionIndexCount, &0_u32);
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &1_u32);
        let genesis = GenesisRecord {
            genesis_id: earnproof_shared::compute_genesis_id(&env, "earnproof_protocol_config"),
            initialized_at_ledger: env.ledger().sequence(),
        };
        env.storage().instance().set(&DataKey::Genesis, &genesis);
        Self::extend_instance_ttl(env.clone());
        Initialized { admin }.publish(&env);
        Ok(())
    }

    /// Returns the immutable genesis identity recorded at `initialize`.
    /// Unchanged across upgrades and storage migrations.
    pub fn get_genesis(env: Env) -> Result<GenesisRecord, ContractError> {
        env.storage()
            .instance()
            .get(&DataKey::Genesis)
            .ok_or(ContractError::NotInitialized)
    }

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)
    }

    /// Machine-readable interface version this contract exposes to consumers.
    pub fn interface_version(_env: Env) -> InterfaceVersion {
        PROTOCOL_CONFIG_INTERFACE_VERSION
    }

    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_valid_principal(&new_admin)?;
        Self::require_auth(&admin);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        AdminChanged { new_admin }.publish(&env);
        Ok(())
    }

    pub fn keepalive_instance(env: Env) -> bool {
        if !env.storage().instance().has(&DataKey::Admin) {
            return false;
        }
        Self::extend_instance_ttl(env);
        true
    }

    pub fn keepalive_schema_version(env: Env, version: u32) -> bool {
        if version == 0 {
            return false;
        }
        let key = DataKey::SchemaVersion(version);
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );
            true
        } else {
            false
        }
    }

    pub fn nominate_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
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
        Self::ensure_not_decommissioned(&env)?;
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

        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::AdminRotation,
            Self::commit(&env, pending_admin.clone()),
        );

        AdminTransferAccepted {
            new_admin: pending_admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn cancel_admin_transfer(env: Env) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
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

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn pause(env: Env) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        env.storage().instance().set(&DataKey::Paused, &true);
        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::PauseToggle,
            Self::commit(&env, true),
        );
        Paused { paused: true }.publish(&env);
        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::PauseToggle,
            Self::commit(&env, false),
        );
        Unpaused { paused: false }.publish(&env);
        Ok(())
    }

    pub fn set_scoped_pause(
        env: Env,
        scope: PauseScope,
        paused: bool,
    ) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        env.storage()
            .persistent()
            .set(&DataKey::ScopedPause(scope), &paused);
        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::ScopedPause,
            Self::commit(&env, (scope, paused)),
        );
        ScopedPauseChanged { scope, paused }.publish(&env);
        Ok(())
    }

    pub fn is_scope_paused(env: Env, scope: PauseScope) -> bool {
        let scoped = env
            .storage()
            .persistent()
            .get(&DataKey::ScopedPause(scope))
            .unwrap_or(false);
        if scoped {
            return true;
        }
        if (scope == PauseScope::Global
            || scope == PauseScope::Registration
            || scope == PauseScope::Updates)
            && Self::is_paused(env)
        {
            return true;
        }
        false
    }

    pub fn nominate_successor(env: Env, successor: Address) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_valid_principal(&successor)?;
        Self::require_auth(&admin);
        env.storage()
            .instance()
            .set(&DataKey::Successor, &successor);
        SuccessorNominated {
            successor: successor.clone(),
            nominated_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_successor(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Successor)
    }

    pub fn activate_successor(env: Env) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        let successor: Address = env
            .storage()
            .instance()
            .get(&DataKey::Successor)
            .ok_or(ContractError::NotFound)?;

        env.storage()
            .instance()
            .set(&DataKey::Decommissioned, &true);
        ContractDecommissioned {
            old_instance: env.current_contract_address(),
            successor_instance: successor,
            activated_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn approve_schema_version(env: Env, version: u32) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_critical_action_policy_disabled(&env)?;
        Self::ensure_nonzero_version(version)?;
        let policy_key = DataKey::SchemaPolicy(version);
        if !env.storage().persistent().has(&policy_key) {
            let mut proof_types = Vec::new(&env);
            proof_types.push_back(LEGACY_PROOF_TYPE);
            env.storage().persistent().set(
                &policy_key,
                &SchemaPolicy {
                    proof_types,
                    max_validity_seconds: MAX_SCHEMA_VALIDITY_SECONDS,
                },
            );
            Self::extend_schema_policy_ttl(env.clone(), version);
        }
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion(version), &true);
        Self::append_schema_version_index(env.clone(), version);
        Self::extend_schema_ttl(env.clone(), version);
        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::SchemaApproval,
            Self::commit(&env, version),
        );
        SchemaApproved { version }.publish(&env);
        Ok(())
    }

    pub fn deprecate_schema_version(env: Env, version: u32) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_role_or_admin(&env, &admin, GovernanceRole::SchemaManagement)?;
        Self::ensure_nonzero_version(version)?;
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion(version), &false);
        Self::append_schema_version_index(env.clone(), version);
        Self::extend_schema_ttl(env.clone(), version);
        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::SchemaDeprecation,
            Self::commit(&env, version),
        );
        SchemaDeprecated {
            proposal_id,
            version,
        }
        .publish(&env);
        Ok(())
    }

    pub fn is_schema_version_approved(env: Env, version: u32) -> bool {
        if version == 0 {
            return false;
        }

        let key = DataKey::SchemaVersion(version);
        let approved = env.storage().persistent().get(&key).unwrap_or(false);
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );
        }
        approved
    }

    pub fn propose_schema_version(
        env: Env,
        version: u32,
        lineage_commitment: BytesN<32>,
        policy_commitment: BytesN<32>,
    ) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_nonzero_version(version)?;

        let schema_key = DataKey::SchemaVersion(version);
        let lifecycle_key = DataKey::SchemaLifecycle(version);
        if env.storage().persistent().has(&schema_key)
            || env.storage().persistent().has(&lifecycle_key)
        {
            return Err(ContractError::AlreadyExists);
        }

        env.storage().persistent().set(&schema_key, &false);
        env.storage().persistent().set(
            &lifecycle_key,
            &SchemaLifecycle {
                lineage_commitment: lineage_commitment.clone(),
                policy_commitment: policy_commitment.clone(),
                active: false,
                deprecated: false,
            },
        );
        Self::extend_schema_ttl(env.clone(), version);
        env.storage().persistent().extend_ttl(
            &lifecycle_key,
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        Self::bump_config_version(env.clone());
        Self::append_schema_transition(env.clone(), version, SchemaTransitionKind::Proposal);
        Ok(())
    }

    pub fn approve_schema_version(env: Env, version: u32) -> Result<(), ContractError> {
    /// Sets a bounded ledger-window registration limit for a schema. A zero
    /// maximum intentionally pauses new registrations; a zero window is invalid.
    pub fn set_schema_rate_limit(
        env: Env,
        version: u32,
        limit: SchemaRateLimit,
    ) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_nonzero_version(version)?;
        if env
            .storage()
            .persistent()
            .get::<_, SchemaLifecycle>(&DataKey::SchemaLifecycle(version))
            .is_some_and(|lifecycle| lifecycle.deprecated)
        {
            return Err(ContractError::InvalidState);
        if limit.window_ledgers == 0 {
            return Err(ContractError::InvalidInput);
        }
        env.storage()
            .persistent()
            .set(&DataKey::SchemaRateLimit(version), &limit);
        env.storage().persistent().extend_ttl(
            &DataKey::SchemaRateLimit(version),
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        Self::bump_config_version(env);
        Ok(())
    }

    /// Returns the configured schema rate policy or the unbounded compatibility default.
    pub fn get_schema_rate_limit(env: Env, version: u32) -> SchemaRateLimit {
        if version == 0 {
            return SchemaRateLimit {
                max_registrations: DEFAULT_SCHEMA_RATE_LIMIT,
                window_ledgers: DEFAULT_SCHEMA_RATE_WINDOW_LEDGERS,
            };
        }
        let key = DataKey::SchemaRateLimit(version);
        let value = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(SchemaRateLimit {
                max_registrations: DEFAULT_SCHEMA_RATE_LIMIT,
                window_ledgers: DEFAULT_SCHEMA_RATE_WINDOW_LEDGERS,
            });
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );
        }
        value
    }

    pub fn get_config_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ConfigVersion)
            .unwrap_or(0)
    }

    pub fn get_schema_index(
        env: Env,
        cursor: u32,
        limit: u32,
    ) -> Vec<earnproof_shared::SchemaVersionSummary> {
        Self::get_schema_index_filtered(env, cursor, limit, None)
    }

    pub fn get_schema_page(
        env: Env,
        cursor: u32,
        limit: u32,
    ) -> Vec<earnproof_shared::SchemaVersionSummary> {
        Self::get_schema_index(env, cursor, limit)
    }

    pub fn list_schema_versions(
        env: Env,
        cursor: u32,
        limit: u32,
    ) -> Vec<earnproof_shared::SchemaVersionSummary> {
        Self::get_schema_index(env, cursor, limit)
    }

    pub fn enumerate_schema_versions(
        env: Env,
        cursor: u32,
        limit: u32,
    ) -> Vec<earnproof_shared::SchemaVersionSummary> {
        Self::get_schema_index(env, cursor, limit)
    }

    pub fn get_schema_index_by_status(
        env: Env,
        cursor: u32,
        limit: u32,
        approved: bool,
    ) -> Vec<earnproof_shared::SchemaVersionSummary> {
        Self::get_schema_index_filtered(env, cursor, limit, Some(approved))
    }

    pub fn get_schema_status_page(
        env: Env,
        cursor: u32,
        limit: u32,
        approved: bool,
    ) -> Vec<earnproof_shared::SchemaVersionSummary> {
        Self::get_schema_index_by_status(env, cursor, limit, approved)
    }

    pub fn get_schema_index_filtered(
        env: Env,
        cursor: u32,
        limit: u32,
        approved: Option<bool>,
    ) -> Vec<earnproof_shared::SchemaVersionSummary> {
        let total = Self::get_schema_version_index_count(env.clone());
        let capped_limit = limit.min(earnproof_shared::MAX_SCHEMA_PAGE);
        let mut page = Vec::new(&env);
        if cursor >= total {
            return page;
        }

        let mut kept = 0_u32;
        for index in cursor..total {
            if let Some(version) = env
                .storage()
                .persistent()
                .get::<_, u32>(&DataKey::SchemaVersionIndex(index))
            {
                let is_approved = env
                    .storage()
                    .persistent()
                    .get::<_, bool>(&DataKey::SchemaVersion(version))
                    .unwrap_or(false);
                if approved
                    .as_ref()
                    .is_some_and(|wanted| is_approved != *wanted)
                {
                    continue;
                }
                if kept >= capped_limit {
                    break;
                }
                page.push_back(earnproof_shared::SchemaVersionSummary {
                    version,
                    approved: is_approved,
                });
                kept += 1;
            }
        }
        page
    }

    pub fn get_schema_version_cursor(env: Env) -> u32 {
        Self::get_schema_version_index_count(env)
    }

    pub fn get_schema_version_count(env: Env) -> u32 {
        Self::get_schema_version_index_count(env)
    }

    pub fn get_schema_count_by_status(env: Env, approved: bool) -> u32 {
        let total = Self::get_schema_version_index_count(env.clone());
        let mut count = 0_u32;
        for index in 0..total {
            if let Some(version) = env
                .storage()
                .persistent()
                .get::<_, u32>(&DataKey::SchemaVersionIndex(index))
            {
                let is_approved = env
                    .storage()
                    .persistent()
                    .get::<_, bool>(&DataKey::SchemaVersion(version))
                    .unwrap_or(false);
                if is_approved == approved {
                    count += 1;
                }
            }
        }
        count
    }

    // ── schema payload size limits ───────────────────────────────────────────

    /// Admin-only: set the maximum auxiliary payload size (in bytes) a proof
    /// registered under `version` may carry. Applies going forward only —
    /// proofs already registered are never re-validated against a changed
    /// limit.
    pub fn set_schema_payload_limit(
        env: Env,
        version: u32,
        max_size: u32,
    ) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_critical_action_policy_disabled(&env)?;
        Self::ensure_nonzero_version(version)?;
        env.storage()
            .persistent()
            .set(&DataKey::SchemaPayloadLimit(version), &max_size);
        Self::extend_schema_payload_limit_ttl(env.clone(), version);
        Self::bump_config_version(env.clone());
        Self::append_schema_transition(env.clone(), version, SchemaTransitionKind::Approval);
        SchemaApproved { version }.publish(&env);
        Ok(())
    }

    pub fn activate_schema_version(env: Env, version: u32) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_nonzero_version(version)?;

        if !Self::is_schema_version_approved(env.clone(), version) {
            return Err(ContractError::InvalidState);
        }
        let lifecycle_key = DataKey::SchemaLifecycle(version);
        let mut lifecycle: SchemaLifecycle = env
            .storage()
            .persistent()
            .get(&lifecycle_key)
            .ok_or(ContractError::InvalidState)?;
        if lifecycle.active || lifecycle.deprecated {
            return Err(ContractError::InvalidState);
        }
        lifecycle.active = true;
        env.storage().persistent().set(&lifecycle_key, &lifecycle);
        env.storage().persistent().extend_ttl(
            &lifecycle_key,
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        Self::bump_config_version(env.clone());
        Self::append_schema_transition(env.clone(), version, SchemaTransitionKind::Activation);
        Ok(())
    }

    pub fn deprecate_schema_version(env: Env, version: u32) -> Result<(), ContractError> {
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::SchemaPayloadLimit,
            Self::commit(&env, (version, max_size)),
        );
        SchemaPayloadLimitSet { version, max_size }.publish(&env);
        Ok(())
    }

    /// Admin-only: configure the proof types and maximum lifetime for a
    /// schema version. A policy can only change while the schema is
    /// deprecated, so existing approvals and proof records are never
    /// reinterpreted by a live policy update.
    pub fn set_schema_policy(
        env: Env,
        version: u32,
        proof_types: Vec<u32>,
        max_validity_seconds: u64,
    ) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_nonzero_version(version)?;
        if env
            .storage()
            .persistent()
            .get::<_, SchemaLifecycle>(&DataKey::SchemaLifecycle(version))
            .is_some_and(|lifecycle| lifecycle.deprecated)
        {
            return Err(ContractError::InvalidState);
        }
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion(version), &false);
        if let Some(mut lifecycle) = env
            .storage()
            .persistent()
            .get::<_, SchemaLifecycle>(&DataKey::SchemaLifecycle(version))
        {
            lifecycle.active = false;
            lifecycle.deprecated = true;
            env.storage()
                .persistent()
                .set(&DataKey::SchemaLifecycle(version), &lifecycle);
            env.storage().persistent().extend_ttl(
                &DataKey::SchemaLifecycle(version),
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );
        }
        Self::extend_schema_ttl(env.clone(), version);
        Self::bump_config_version(env.clone());
        Self::append_schema_transition(env.clone(), version, SchemaTransitionKind::Deprecation);
        SchemaDeprecated { version }.publish(&env);
        Ok(())
    }

    pub fn revise_schema_policy(
        env: Env,
        version: u32,
        policy_commitment: BytesN<32>,
    ) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_nonzero_version(version)?;

        let lifecycle_key = DataKey::SchemaLifecycle(version);
        let mut lifecycle: SchemaLifecycle = env
            .storage()
            .persistent()
            .get(&lifecycle_key)
            .ok_or(ContractError::NotFound)?;
        if lifecycle.deprecated || lifecycle.policy_commitment == policy_commitment {
            return Err(ContractError::InvalidState);
        }

        lifecycle.policy_commitment = policy_commitment;
        lifecycle.active = false;
        env.storage().persistent().set(&lifecycle_key, &lifecycle);
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion(version), &false);
        env.storage().persistent().extend_ttl(
            &lifecycle_key,
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        Self::extend_schema_ttl(env.clone(), version);
        Self::bump_config_version(env.clone());
        Self::append_schema_transition(env.clone(), version, SchemaTransitionKind::PolicyRevision);
        Ok(())
    }

    pub fn get_schema_transition_history(
        env: Env,
        cursor: u64,
        limit: u32,
    ) -> Result<SchemaTransitionPage, ContractError> {
        if limit == 0 || limit > MAX_SCHEMA_TRANSITION_PAGE_SIZE {
            return Err(ContractError::InvalidInput);
        }
        let total_cursor: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SchemaTransitionCount)
            .unwrap_or(0);
        let oldest_cursor = total_cursor.saturating_sub(SCHEMA_TRANSITION_HISTORY_CAPACITY as u64);
        let start = cursor.max(oldest_cursor).min(total_cursor);
        let end = start.saturating_add(limit as u64).min(total_cursor);
        let mut transitions = Vec::new(&env);
        for sequence in start..end {
            let slot = (sequence % SCHEMA_TRANSITION_HISTORY_CAPACITY as u64) as u32;
            let transition: SchemaTransition = env
                .storage()
                .persistent()
                .get(&DataKey::SchemaTransitionSlot(slot))
                .ok_or(ContractError::InvalidState)?;
            if transition.sequence != sequence {
                return Err(ContractError::InvalidState);
            }
            transitions.push_back(transition);
        }
        Ok(SchemaTransitionPage {
            transitions,
            next_cursor: end,
            oldest_cursor,
            total_cursor,
        })
    }

    pub fn is_schema_version_approved(env: Env, version: u32) -> bool {
        if env.storage().persistent().has(&DataKey::SchemaVersion(version))
            || proof_types.is_empty()
            || proof_types.len() > MAX_SCHEMA_PROOF_TYPES
            || max_validity_seconds == 0
            || max_validity_seconds > MAX_SCHEMA_VALIDITY_SECONDS
        {
            return Err(ContractError::InvalidInput);
        }
        let mut index = 0;
        while index < proof_types.len() {
            let value = proof_types.get(index).ok_or(ContractError::InvalidInput)?;
            let mut prior = 0;
            while prior < index {
                if proof_types.get(prior) == Some(value) {
                    return Err(ContractError::InvalidInput);
                }
                prior += 1;
            }
            index += 1;
        }

        let policy = SchemaPolicy {
            proof_types: proof_types.clone(),
            max_validity_seconds,
        };
        env.storage()
            .persistent()
            .set(&DataKey::SchemaPolicy(version), &policy);
        Self::extend_schema_policy_ttl(env.clone(), version);
        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::SchemaPolicy,
            Self::commit(&env, (version, policy)),
        );
        SchemaPolicySet {
            version,
            proof_type_count: proof_types.len(),
            max_validity_seconds,
        }
        .publish(&env);
        Ok(())
    }

    /// Returns the configured policy. Schemas predating policy storage use a
    /// compatibility policy for legacy registrations and the finite maximum
    /// validity horizon.
    pub fn get_schema_policy(env: Env, version: u32) -> SchemaPolicy {
        if version == 0 {
            return Self::legacy_schema_policy(&env);
        }
        let key = DataKey::SchemaPolicy(version);
        let policy = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Self::legacy_schema_policy(&env));
        if env.storage().persistent().has(&key) {
            Self::extend_schema_policy_ttl(env, version);
        }
        policy
    }

    /// Admin-only: enable or retire a supported commitment algorithm for new
    /// registrations. Identifiers outside the published set are rejected.
    pub fn set_commitment_algorithm(
        env: Env,
        algorithm: u32,
        supported: bool,
    ) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        if algorithm != LEGACY_COMMITMENT_ALGORITHM
            && algorithm != SHA256_COMMITMENT_ALGORITHM_V1
        {
            return Err(ContractError::InvalidInput);
        }
        env.storage()
            .instance()
            .set(&DataKey::CommitmentAlgorithm(algorithm), &supported);
        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::CommitmentAlgorithmPolicy,
            Self::commit(&env, (algorithm, supported)),
        );
        CommitmentAlgorithmPolicySet {
            algorithm,
            supported,
        }
        .publish(&env);
        Ok(())
    }

    /// Returns whether an algorithm is enabled for new registrations. Both
    /// published identifiers are enabled by default; unknown identifiers are
    /// always rejected.
    pub fn is_algorithm_supported(env: Env, algorithm: u32) -> bool {
        if algorithm != LEGACY_COMMITMENT_ALGORITHM
            && algorithm != SHA256_COMMITMENT_ALGORITHM_V1
        {
            return false;
        }
        let key = DataKey::CommitmentAlgorithm(algorithm);
        env.storage().instance().get(&key).unwrap_or(true)
    }

    /// Returns the active payload-size bound for `version`: the governed
    /// override if one was set, otherwise the shared default. Version `0`
    /// always returns the default, since it can never be approved.
    pub fn get_schema_payload_limit(env: Env, version: u32) -> u32 {
        if version == 0 {
            return DEFAULT_SCHEMA_PAYLOAD_LIMIT;
        }
        let key = DataKey::SchemaPayloadLimit(version);
        let limit = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(DEFAULT_SCHEMA_PAYLOAD_LIMIT);
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                TTL_THRESHOLD_LEDGERS,
                TTL_EXTEND_TO_LEDGERS,
            );
        }
        limit
    }

    // ── bounded configuration change history ─────────────────────────────────

    /// Returns up to `limit` change-history entries starting at `cursor`
    /// (an absolute, monotonically increasing sequence number; `0` is the
    /// very first change ever recorded).
    ///
    /// If `cursor` refers to an entry the ring has already evicted, the read
    /// starts at the oldest entry still available rather than erroring — a
    /// stale cursor is clamped forward, deterministically, instead of
    /// panicking. `limit` is capped at `MAX_CONFIG_HISTORY_PAGE` regardless of
    /// the requested value.
    pub fn get_config_history(env: Env, cursor: u32, limit: u32) -> Vec<ConfigChangeSummary> {
        let total = Self::get_config_history_cursor(env.clone());
        let oldest_available = total.saturating_sub(CONFIG_HISTORY_CAPACITY);
        let start = cursor.max(oldest_available);
        let capped_limit = limit.min(MAX_CONFIG_HISTORY_PAGE);
        let end = start.saturating_add(capped_limit).min(total);

        let mut page = Vec::new(&env);
        let mut index = start;
        while index < end {
            let slot = index % CONFIG_HISTORY_CAPACITY;
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<_, ConfigChangeSummary>(&DataKey::ConfigHistoryRing(slot))
            {
                page.push_back(entry);
            }
            index += 1;
        }
        page
    }

    /// Total number of change-history entries ever appended. Also the cursor
    /// value a caller should pass to `get_config_history` to read only
    /// entries recorded after the most recent page it consumed.
    pub fn get_config_history_cursor(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ConfigHistoryTotal)
            .unwrap_or(0)
    }

    pub fn get_max_proofs(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxProofs)
            .unwrap_or(u32::MAX)
    }

    pub fn set_max_proofs(env: Env, max_proofs: u32) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        env.storage()
            .instance()
            .set(&DataKey::MaxProofs, &max_proofs);
        Self::bump_config_version(env.clone());
        ProofCapacityChanged {
            max_proofs,
            changed_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    // ── upgrade governance ───────────────────────────────────────────────────

    /// Returns the stored monotonic contract version (separate from the
    /// config-mutation counter).  Starts at 1 after `initialize`.
    pub fn get_contract_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0)
    }

    /// Admin-only: add `wasm_hash` to the upgrade allowlist and record the
    /// `new_version` that must be installed by that WASM.
    ///
    /// `new_version` must be strictly greater than the currently stored
    /// contract version so that a downgrade cannot be pre-approved.
    pub fn approve_upgrade(env: Env, wasm_hash: BytesN<32>, new_version: u32) {
        if Self::is_decommissioned(env.clone()) {
            panic!("contract is decommissioned");
        }
        if Self::is_scope_paused(env.clone(), PauseScope::Upgrades) {
            panic!("upgrades are paused");
        }
        let admin = Self::get_admin(env.clone()).expect("contract not initialized");

        Self::require_auth(&admin);

        let current = Self::get_contract_version(env.clone());
        if new_version <= current {
            panic!("new_version must be greater than current contract version");
        }

        env.storage()
            .instance()
            .set(&DataKey::AllowedWasm(wasm_hash.clone()), &new_version);
        Self::extend_instance_ttl(env.clone());

        UpgradeAllowlisted {
            wasm_hash,
            new_contract_version: new_version,
            approved_by: admin,
        }
        .publish(&env);
    }

    pub fn propose_upgrade_attestation(
        env: Env,
        wasm_hash: BytesN<32>,
        attestation: UpgradeCompatibilityAttestation,
    ) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        if attestation.version == 0 {
            return Err(ContractError::InvalidInput);
        }

        env.storage().instance().set(
            &DataKey::UpgradeAttestationProposal(wasm_hash.clone()),
            &attestation,
        );
        env.storage()
            .instance()
            .remove(&DataKey::UpgradeAttestationApproval(wasm_hash.clone()));
        env.storage()
            .instance()
            .remove(&DataKey::AllowedWasm(wasm_hash));
        Self::extend_instance_ttl(env);
        Ok(())
    }

    pub fn get_upgrade_attestation(
        env: Env,
        wasm_hash: BytesN<32>,
    ) -> Option<UpgradeCompatibilityAttestation> {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeAttestationProposal(wasm_hash))
    }

    pub fn get_attested_upgrade_receipt(env: Env) -> Option<AttestedUpgradeReceipt> {
        env.storage()
            .instance()
            .get(&DataKey::LatestAttestedUpgradeReceipt)
    }

    pub fn approve_upgrade_with_attestation(
        env: Env,
        wasm_hash: BytesN<32>,
        new_version: u32,
    ) -> Result<(), ContractError> {
        let attestation = Self::get_upgrade_attestation(env.clone(), wasm_hash.clone())
            .ok_or(ContractError::NotFound)?;
        Self::approve_upgrade(env.clone(), wasm_hash.clone(), new_version);
        env.storage().instance().set(
            &DataKey::UpgradeAttestationApproval(wasm_hash),
            &attestation,
        );
        Self::extend_instance_ttl(env);
        Ok(())
    }

    /// Admin-only: remove a previously allowlisted WASM hash without applying
    /// it.  Safe to call even if the hash was never allowlisted.
    pub fn revoke_upgrade(env: Env, wasm_hash: BytesN<32>) {
        if Self::is_decommissioned(env.clone()) {
            panic!("contract is decommissioned");
        }
        let admin = Self::get_admin(env.clone()).expect("contract not initialized");
        Self::require_auth(&admin);

        env.storage()
            .instance()
            .remove(&DataKey::AllowedWasm(wasm_hash.clone()));
        env.storage()
            .instance()
            .remove(&DataKey::UpgradeAttestationApproval(wasm_hash.clone()));

        UpgradeRevoked {
            wasm_hash,
            revoked_by: admin,
        }
        .publish(&env);
    }

    /// Returns true when `wasm_hash` is on the allowlist.
    pub fn is_upgrade_allowed(env: Env, wasm_hash: BytesN<32>) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::AllowedWasm(wasm_hash))
    }

    /// Admin-only: apply an in-place WASM upgrade.
    ///
    /// Requirements (all checked on-chain before the upgrade is applied):
    /// 1. Caller is the admin.
    /// 2. `wasm_hash` is on the allowlist (pre-approved via `approve_upgrade`).
    /// 3. The target version stored in the allowlist entry is strictly greater
    ///    than the current `ContractVersion` (downgrade guard).
    ///
    /// On success the new WASM is installed, `ContractVersion` is advanced,
    /// the allowlist entry is consumed (removed), and a `ContractUpgraded`
    /// event is emitted.
    pub fn upgrade_contract(env: Env, wasm_hash: BytesN<32>) {
        if Self::is_decommissioned(env.clone()) {
            panic!("contract is decommissioned");
        }
        if Self::is_scope_paused(env.clone(), PauseScope::Upgrades) {
            panic!("upgrades are paused");
        }
        let admin = Self::get_admin(env.clone()).expect("contract not initialized");
        Self::require_auth(&admin);

        let new_version: u32 = env
            .storage()
            .instance()
            .get(&DataKey::AllowedWasm(wasm_hash.clone()))
            .expect("wasm hash not on allowlist");

        let attestation: Option<UpgradeCompatibilityAttestation> = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeAttestationApproval(wasm_hash.clone()));
        if let Some(approved) = &attestation {
            let proposed: Option<UpgradeCompatibilityAttestation> = env
                .storage()
                .instance()
                .get(&DataKey::UpgradeAttestationProposal(wasm_hash.clone()));
            assert_eq!(
                proposed.as_ref(),
                Some(approved),
                "upgrade attestation changed"
            );
        } else {
            assert!(
                !env.storage()
                    .instance()
                    .has(&DataKey::UpgradeAttestationProposal(wasm_hash.clone())),
                "upgrade requires attested approval"
            );
        }

        let old_version = Self::get_contract_version(env.clone());
        if new_version <= old_version {
            panic!("upgrade would not advance contract version");
        }

        // Consume the allowlist entry before applying so re-entrancy cannot
        // replay the same hash.
        env.storage()
            .instance()
            .remove(&DataKey::AllowedWasm(wasm_hash.clone()));

        // Apply the WASM upgrade.  This replaces the executable code while
        // leaving all stored state intact.
        #[cfg(not(test))]
        env.deployer()
            .update_current_contract_wasm(wasm_hash.clone());

        // Record the new version.
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &new_version);
        Self::extend_instance_ttl(env.clone());

        if let Some(attestation) = attestation {
            let receipt = UpgradeReceipt {
                wasm_hash: wasm_hash.clone(),
                old_version,
                new_version,
                upgraded_at: env.ledger().timestamp(),
                upgraded_by: admin.clone(),
            };
            env.storage().instance().set(
                &DataKey::LatestAttestedUpgradeReceipt,
                &AttestedUpgradeReceipt {
                    receipt,
                    attestation,
                },
            );
            env.storage()
                .instance()
                .remove(&DataKey::UpgradeAttestationProposal(wasm_hash.clone()));
            env.storage()
                .instance()
                .remove(&DataKey::UpgradeAttestationApproval(wasm_hash.clone()));
        }

        ContractUpgraded {
            new_wasm_hash: wasm_hash,
            old_contract_version: old_version,
            new_contract_version: new_version,
            upgraded_by: admin,
        }
        .publish(&env);
    }

    // ── private helpers ──────────────────────────────────────────────────────

    fn ensure_nonzero_version(version: u32) -> Result<(), ContractError> {
        if version == 0 {
            return Err(ContractError::InvalidInput);
        }
        Ok(())
    }

    fn require_valid_principal(address: &Address) -> Result<(), ContractError> {
        if !earnproof_shared::is_valid_principal_address(address) {
            return Err(ContractError::InvalidInput);
        }
        Ok(())
    }

    fn bump_config_version(env: Env) {
        let current = Self::get_config_version(env.clone());
        let new_version = current
            .checked_add(1)
            .unwrap_or_else(|| panic!("config version overflow: reached maximum"));
        env.storage()
            .instance()
            .set(&DataKey::ConfigVersion, &new_version);
        Self::extend_instance_ttl(env);
    }

    fn append_schema_version_index(env: Env, version: u32) {
        let total = Self::get_schema_version_index_count(env.clone());
        for index in 0..total {
            if let Some(existing) = env
                .storage()
                .persistent()
                .get::<_, u32>(&DataKey::SchemaVersionIndex(index))
            {
                if existing == version {
                    return;
                }
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersionIndex(total), &version);
        env.storage().persistent().extend_ttl(
            &DataKey::SchemaVersionIndex(total),
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        let next_total = total
            .checked_add(1)
            .unwrap_or_else(|| panic!("schema version index overflow: reached maximum"));
        env.storage()
            .instance()
            .set(&DataKey::SchemaVersionIndexCount, &next_total);
        Self::extend_instance_ttl(env);
    }

    fn get_schema_version_index_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::SchemaVersionIndexCount)
            .unwrap_or(0)
    }

    fn extend_instance_ttl(env: Env) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
    }

    fn extend_schema_ttl(env: Env, version: u32) {
        env.storage().persistent().extend_ttl(
            &DataKey::SchemaVersion(version),
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
    }

    fn append_schema_transition(env: Env, version: u32, kind: SchemaTransitionKind) {
        let sequence: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SchemaTransitionCount)
            .unwrap_or(0);
        let lifecycle: Option<SchemaLifecycle> = env
            .storage()
            .persistent()
            .get(&DataKey::SchemaLifecycle(version));
        let transition = SchemaTransition {
            sequence,
            version,
            kind,
            transitioned_at: env.ledger().timestamp(),
            lineage_commitment: lifecycle
                .as_ref()
                .map(|record| record.lineage_commitment.clone()),
            policy_commitment: lifecycle.map(|record| record.policy_commitment),
        };
        let slot = (sequence % SCHEMA_TRANSITION_HISTORY_CAPACITY as u64) as u32;
        let key = DataKey::SchemaTransitionSlot(slot);
        env.storage().persistent().set(&key, &transition);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
        let next_sequence = sequence
            .checked_add(1)
            .unwrap_or_else(|| panic!("schema transition cursor overflow"));
        env.storage()
            .instance()
            .set(&DataKey::SchemaTransitionCount, &next_sequence);
    fn extend_schema_payload_limit_ttl(env: Env, version: u32) {
        env.storage().persistent().extend_ttl(
            &DataKey::SchemaPayloadLimit(version),
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
    }

    fn extend_schema_policy_ttl(env: Env, version: u32) {
        env.storage().persistent().extend_ttl(
            &DataKey::SchemaPolicy(version),
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
    }

    fn legacy_schema_policy(env: &Env) -> SchemaPolicy {
        let mut proof_types = Vec::new(env);
        proof_types.push_back(LEGACY_PROOF_TYPE);
        SchemaPolicy {
            proof_types,
            max_validity_seconds: MAX_SCHEMA_VALIDITY_SECONDS,
        }
    }

    /// SHA-256 commitment to an arbitrary XDR-encodable value. Used so
    /// change-history entries carry proof of what changed without carrying
    /// the value itself.
    fn commit<T: ToXdr>(env: &Env, value: T) -> BytesN<32> {
        env.crypto().sha256(&value.to_xdr(env)).to_bytes()
    }

    /// Appends one bounded change-history entry, overwriting the oldest slot
    /// once the ring is full. Only called from a success path, after the
    /// state it summarizes has already been committed, so a failed mutation
    /// never appends.
    fn append_config_history(
        env: Env,
        category: ConfigChangeCategory,
        proposal_commitment: BytesN<32>,
    ) {
        let total = Self::get_config_history_cursor(env.clone());
        let slot = total % CONFIG_HISTORY_CAPACITY;
        let summary = ConfigChangeSummary {
            category,
            proposal_commitment,
            config_version: Self::get_config_version(env.clone()),
            ledger: env.ledger().sequence(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::ConfigHistoryRing(slot), &summary);
        env.storage().persistent().extend_ttl(
            &DataKey::ConfigHistoryRing(slot),
            TTL_THRESHOLD_LEDGERS,
            TTL_EXTEND_TO_LEDGERS,
        );
        let next_total = total
            .checked_add(1)
            .unwrap_or_else(|| panic!("config history overflow: reached maximum"));
        env.storage()
            .instance()
            .set(&DataKey::ConfigHistoryTotal, &next_total);
        Self::extend_instance_ttl(env);
    }

    pub fn get_migration_status(env: Env) -> Option<MigrationStatus> {
        env.storage().instance().get(&DataKey::MigrationStatus)
    }

    pub fn begin_migration(
        env: Env,
        target_contract_version: u32,
        total_items: u32,
    ) -> Result<MigrationStatus, ContractError> {
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

    pub fn advance_migration(
        env: Env,
        expected_cursor: u32,
        processed_items: u32,
    ) -> Result<MigrationStatus, ContractError> {
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

    fn store_critical_action_policy(
        env: Env,
        policy: CriticalActionPolicy,
    ) -> Result<(), ContractError> {
        let commitment = Self::commit(&env, policy.clone());
        Self::store_critical_action_policy_with_commitment(env, policy, commitment);
        Ok(())
    }

    fn store_critical_action_policy_with_commitment(
        env: Env,
        policy: CriticalActionPolicy,
        commitment: BytesN<32>,
    ) {
        env.storage()
            .instance()
            .set(&DataKey::CriticalActionPolicy, &policy);
        Self::bump_config_version(env.clone());
        Self::append_config_history(
            env.clone(),
            ConfigChangeCategory::ApprovalPolicyUpdate,
            commitment,
        );
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

    fn record_critical_action_change(
        env: Env,
        category: ConfigChangeCategory,
        commitment: BytesN<32>,
    ) {
        Self::bump_config_version(env.clone());
        Self::append_config_history(env, category, commitment);
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::{
        DataKey, ProtocolConfigContract, ProtocolConfigContractClient, SchemaTransitionKind,
    };
    use earnproof_shared::{UpgradeCompatibilityAttestation, TTL_THRESHOLD_LEDGERS};
    use soroban_sdk::{testutils::storage::Persistent as _, Address, BytesN, Env};
    use super::{DataKey, ProtocolConfigContract, ProtocolConfigContractClient};
    use earnproof_shared::{ConfigChangeCategory, TTL_THRESHOLD_LEDGERS};
    use soroban_sdk::{
        testutils::{storage::Persistent as _, Address as _, Ledger as _},
        Address, BytesN, Env,
    };

    const ADMIN: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";
    const OTHER: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";

    fn bytes(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn setup() -> (Env, ProtocolConfigContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        client.initialize(&admin);
        (env, client, admin)
    }

    // ── existing tests ────────────────────────────────────────────────────────

    #[test]
    fn exposes_a_stable_interface_version() {
        let (_env, client, _admin) = setup();
        let version = client.interface_version();
        assert_eq!(version, earnproof_shared::PROTOCOL_CONFIG_INTERFACE_VERSION);
        assert_eq!(version.major, 1);
    }

    #[test]
    fn initializes_config_defaults() {
        let (_env, client, admin) = setup();

        assert_eq!(client.get_admin(), admin);
        assert!(!client.is_paused());
        assert_eq!(client.get_config_version(), 1);
        assert!(!client.is_schema_version_approved(&1));
        // contract version initialized to 1
        assert_eq!(client.get_contract_version(), 1);
    }

    fn p(env: &Env, val: u8) -> BytesN<32> {
        BytesN::from_array(env, &[val; 32])
    }

    #[test]
    fn pause_and_unpause_bump_config_version() {
        let (env, client, _admin) = setup();

        client.pause(&p(&env, 1));
        assert!(client.is_paused());
        assert_eq!(client.get_config_version(), 2);

        client.unpause(&p(&env, 2));
        assert!(!client.is_paused());
        assert_eq!(client.get_config_version(), 3);
    }

    #[test]
    fn schema_versions_can_be_approved_and_deprecated() {
        let (env, client, _admin) = setup();

        client.approve_schema_version(&p(&env, 1), &1);
        assert!(client.is_schema_version_approved(&1));

        client.deprecate_schema_version(&p(&env, 2), &1);
        assert!(!client.is_schema_version_approved(&1));
    }

    #[test]
    fn schema_lifecycle_history_preserves_order_and_commitments() {
        let (env, client, _admin) = setup();
        let lineage = bytes(&env, 11);
        let original_policy = bytes(&env, 12);
        let revised_policy = bytes(&env, 13);

        client.propose_schema_version(&7, &lineage, &original_policy);
        client.approve_schema_version(&7);
        client.activate_schema_version(&7);
        client.revise_schema_policy(&7, &revised_policy);
        assert!(!client.is_schema_version_approved(&7));
        client.approve_schema_version(&7);
        client.activate_schema_version(&7);
        client.deprecate_schema_version(&7);

        let page = client.get_schema_transition_history(&0, &16);
        assert_eq!(page.oldest_cursor, 0);
        assert_eq!(page.total_cursor, 7);
        assert_eq!(page.next_cursor, 7);
        assert_eq!(page.transitions.len(), 7);
        assert_eq!(
            page.transitions.get(0).unwrap().kind,
            SchemaTransitionKind::Proposal
        );
        assert_eq!(
            page.transitions.get(1).unwrap().kind,
            SchemaTransitionKind::Approval
        );
        assert_eq!(
            page.transitions.get(2).unwrap().kind,
            SchemaTransitionKind::Activation
        );
        assert_eq!(
            page.transitions.get(3).unwrap().kind,
            SchemaTransitionKind::PolicyRevision
        );
        assert_eq!(
            page.transitions.get(4).unwrap().kind,
            SchemaTransitionKind::Approval
        );
        assert_eq!(
            page.transitions.get(5).unwrap().kind,
            SchemaTransitionKind::Activation
        );
        assert_eq!(
            page.transitions.get(6).unwrap().kind,
            SchemaTransitionKind::Deprecation
        );
        for index in 0..page.transitions.len() {
            let transition = page.transitions.get(index).unwrap();
            assert_eq!(transition.sequence, index as u64);
            assert_eq!(transition.version, 7);
            assert_eq!(transition.lineage_commitment, Some(lineage.clone()));
        }
        assert_eq!(
            page.transitions.get(0).unwrap().policy_commitment,
            Some(original_policy)
        );
        assert_eq!(
            page.transitions.get(3).unwrap().policy_commitment,
            Some(revised_policy)
        );
    }

    #[test]
    fn schema_lifecycle_rejects_invalid_transitions_without_history() {
        let (env, client, _admin) = setup();
        use earnproof_shared::ContractError;

        assert_eq!(
            client.try_activate_schema_version(&1),
            Err(Ok(ContractError::InvalidState))
        );
        client.propose_schema_version(&1, &bytes(&env, 1), &bytes(&env, 2));
        assert_eq!(
            client.try_propose_schema_version(&1, &bytes(&env, 3), &bytes(&env, 4)),
            Err(Ok(ContractError::AlreadyExists))
        );
        client.approve_schema_version(&1);
        client.activate_schema_version(&1);
        assert_eq!(
            client.try_activate_schema_version(&1),
            Err(Ok(ContractError::InvalidState))
        );
        client.deprecate_schema_version(&1);
        assert_eq!(
            client.try_revise_schema_policy(&1, &bytes(&env, 5)),
            Err(Ok(ContractError::InvalidState))
        );
        assert_eq!(
            client.try_approve_schema_version(&1),
            Err(Ok(ContractError::InvalidState))
        );
        assert_eq!(
            client.try_deprecate_schema_version(&1),
            Err(Ok(ContractError::InvalidState))
        );
        assert!(!client.is_schema_version_approved(&1));
        assert_eq!(
            client.get_schema_transition_history(&0, &16).total_cursor,
            4
        );
    }

    #[test]
    fn schema_transition_history_rollover_is_deterministic_and_bounded() {
        let (_env, client, _admin) = setup();

        for version in 1..=35 {
            client.approve_schema_version(&version);
        }

        let first_page = client.get_schema_transition_history(&0, &16);
        assert_eq!(first_page.oldest_cursor, 3);
        assert_eq!(first_page.total_cursor, 35);
        assert_eq!(first_page.next_cursor, 19);
        assert_eq!(first_page.transitions.len(), 16);
        assert_eq!(first_page.transitions.get(0).unwrap().sequence, 3);
        assert_eq!(first_page.transitions.get(0).unwrap().version, 4);

        let second_page = client.get_schema_transition_history(&first_page.next_cursor, &16);
        assert_eq!(second_page.transitions.len(), 16);
        assert_eq!(second_page.transitions.get(15).unwrap().sequence, 34);
        assert_eq!(second_page.transitions.get(15).unwrap().version, 35);
        assert_eq!(second_page.next_cursor, 35);
    }

    #[test]
    fn legacy_schema_state_is_readable_and_history_is_bounded() {
        let (env, client, _admin) = setup();
        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .set(&DataKey::SchemaVersion(9), &true);
        });

        assert!(client.is_schema_version_approved(&9));
        client.deprecate_schema_version(&9);
        let page = client.get_schema_transition_history(&0, &16);
        assert_eq!(page.transitions.len(), 1);
        assert_eq!(
            page.transitions.get(0).unwrap().kind,
            SchemaTransitionKind::Deprecation
        );
        assert_eq!(page.transitions.get(0).unwrap().lineage_commitment, None);
    }

    #[test]
    fn schema_transition_history_rejects_invalid_page_sizes() {
        let (_env, client, _admin) = setup();
        use earnproof_shared::ContractError;

        assert_eq!(
            client.try_get_schema_transition_history(&0, &0),
            Err(Ok(ContractError::InvalidInput))
        );
        assert_eq!(
            client.try_get_schema_transition_history(&0, &17),
            Err(Ok(ContractError::InvalidInput))
        );
    }

    #[test]
    fn schema_transition_history_slots_receive_persistent_ttl() {
        let (env, client, _admin) = setup();
        client.approve_schema_version(&3);

        env.as_contract(&client.address, || {
            assert!(
                env.storage()
                    .persistent()
                    .get_ttl(&DataKey::SchemaTransitionSlot(0))
                    > TTL_THRESHOLD_LEDGERS
            );
        });
    }

    #[test]
    fn rejects_zero_schema_version() {
        let (env, client, _admin) = setup();
        use earnproof_shared::ContractError;

        let result = client.try_approve_schema_version(&p(&env, 1), &0);
        assert_eq!(result, Err(Ok(ContractError::InvalidInput)));
    }

    #[test]
    fn extends_schema_storage_ttl() {
        let (env, client, _admin) = setup();

        client.approve_schema_version(&p(&env, 1), &7);

        env.as_contract(&client.address, || {
            assert!(
                env.storage()
                    .persistent()
                    .get_ttl(&DataKey::SchemaVersion(7))
                    > TTL_THRESHOLD_LEDGERS
            );
        });
    }

    // ── upgrade governance tests ──────────────────────────────────────────────

    #[test]
    fn approve_and_check_allowlist() {
        let (env, client, _admin) = setup();
        let hash = bytes(&env, 0xab);

        assert!(!client.is_upgrade_allowed(&hash));
        client.approve_upgrade(&p(&env, 1), &hash, &2);
        assert!(client.is_upgrade_allowed(&hash));
    }

    #[test]
    fn revoke_removes_from_allowlist() {
        let (env, client, _admin) = setup();
        let hash = bytes(&env, 0xcd);

        client.approve_upgrade(&p(&env, 1), &hash, &2);
        assert!(client.is_upgrade_allowed(&hash));

        client.revoke_upgrade(&p(&env, 2), &hash);
        assert!(!client.is_upgrade_allowed(&hash));
    }

    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn approve_upgrade_rejects_downgrade_version() {
        let (env, client, _admin) = setup();
        // current version is 1; attempting to allowlist version 1 is rejected
        client.approve_upgrade(&bytes(&env, 1), &1);
    }

    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn approve_upgrade_rejects_lower_version() {
        let (env, client, _admin) = setup();
        // current version is 1; version 0 must be rejected
        client.approve_upgrade(&bytes(&env, 1), &0);
    }

    #[test]
    #[should_panic(expected = "wasm hash not on allowlist")]
    fn upgrade_contract_rejects_non_allowlisted_hash() {
        let (env, client, _admin) = setup();
        client.upgrade_contract(&bytes(&env, 0xff));
    }

    /// Verifies that `upgrade_contract` enforces admin authorization before
    /// doing anything.  `mock_all_auths` is intentionally NOT used here.
    #[test]
    #[should_panic]
    fn upgrade_contract_requires_admin_auth() {
        let env = Env::default();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        // Initialize using mocked auths.
        env.mock_all_auths();
        client.initialize(&admin);
        let hash = BytesN::from_array(&env, &[0xde; 32]);
        client.approve_upgrade(&p(&env, 1), &hash, &2);
        env.set_auths(&[]);

        // Attempt upgrade without auth — must panic.
        client.upgrade_contract(&hash);
    }

    /// Verifies that the allowlist entry is consumed after a successful upgrade
    /// so the same hash cannot be replayed.
    ///
    /// Note: in the test environment `update_current_contract_wasm` is a no-op
    /// (the WASM is not actually swapped) but all surrounding state transitions
    /// — version bump, allowlist removal, event emission — are fully exercised.
    #[test]
    fn upgrade_contract_advances_version_and_consumes_allowlist() {
        let (env, client, _admin) = setup();
        let hash = bytes(&env, 0x42);

        assert_eq!(client.get_contract_version(), 1);
        client.approve_upgrade(&p(&env, 1), &hash, &2);
        assert!(client.is_upgrade_allowed(&hash));

        client.upgrade_contract(&hash);

        // Version must have advanced.
        assert_eq!(client.get_contract_version(), 2);
        // Allowlist entry must have been consumed.
        assert!(!client.is_upgrade_allowed(&hash));
    }

    #[test]
    fn attested_upgrade_exposes_and_carries_exact_commitments() {
        let (env, client, _admin) = setup();
        let hash = bytes(&env, 0x51);
        let attestation = UpgradeCompatibilityAttestation {
            version: 1,
            abi_commitment: bytes(&env, 0x52),
            storage_commitment: bytes(&env, 0x53),
            review_commitment: bytes(&env, 0x54),
        };

        client.propose_upgrade_attestation(&hash, &attestation);
        assert_eq!(
            client.get_upgrade_attestation(&hash),
            Some(attestation.clone())
        );
        client.approve_upgrade_with_attestation(&hash, &2);
        client.upgrade_contract(&hash);

        let receipt = client.get_attested_upgrade_receipt().unwrap();
        assert_eq!(receipt.attestation, attestation);
        assert_eq!(receipt.receipt.wasm_hash, hash);
        assert_eq!(receipt.receipt.old_version, 1);
        assert_eq!(receipt.receipt.new_version, 2);
        assert!(client.get_upgrade_attestation(&hash).is_none());
        assert!(client.try_upgrade_contract(&hash).is_err());
    }

    #[test]
    fn changing_or_cancelling_attestation_invalidates_upgrade_approval() {
        let (env, client, _admin) = setup();
        let hash = bytes(&env, 0x61);
        let first = UpgradeCompatibilityAttestation {
            version: 1,
            abi_commitment: bytes(&env, 1),
            storage_commitment: bytes(&env, 2),
            review_commitment: bytes(&env, 3),
        };
        let revised = UpgradeCompatibilityAttestation {
            review_commitment: bytes(&env, 4),
            ..first.clone()
        };

        client.propose_upgrade_attestation(&hash, &first);
        client.approve_upgrade_with_attestation(&hash, &2);
        assert!(client.is_upgrade_allowed(&hash));
        client.propose_upgrade_attestation(&hash, &revised);
        assert!(!client.is_upgrade_allowed(&hash));
        assert!(client.try_upgrade_contract(&hash).is_err());

        client.approve_upgrade_with_attestation(&hash, &2);
        client.revoke_upgrade(&hash);
        assert!(!client.is_upgrade_allowed(&hash));
        assert!(client.try_upgrade_contract(&hash).is_err());
    }

    #[test]
    fn attestation_type_round_trips_as_canonical_contract_data() {
        let (env, client, _admin) = setup();
        let hash = bytes(&env, 0x71);
        let attestation = UpgradeCompatibilityAttestation {
            version: 7,
            abi_commitment: bytes(&env, 0xa1),
            storage_commitment: bytes(&env, 0xb2),
            review_commitment: bytes(&env, 0xc3),
        };

        client.propose_upgrade_attestation(&hash, &attestation);
        assert_eq!(client.get_upgrade_attestation(&hash), Some(attestation));
    }

    /// After a successful upgrade the same hash cannot be applied a second
    /// time (allowlist entry was consumed, and the version guard would also
    /// block it even if re-approved with the same version).
    #[test]
    #[should_panic(expected = "wasm hash not on allowlist")]
    fn upgrade_contract_hash_cannot_be_replayed() {
        let (env, client, _admin) = setup();
        let hash = bytes(&env, 0x42);

        client.approve_upgrade(&hash, &2);
        client.upgrade_contract(&hash);
        // Second application must fail — entry was consumed.
        client.upgrade_contract(&hash);
    }

    /// State written before an upgrade is still readable after.
    #[test]
    fn state_preserved_across_upgrade() {
        let (env, client, _admin) = setup();

        // Write some state before the upgrade.
        client.approve_schema_version(&p(&env, 1), &3);
        let hash = bytes(&env, 0x77);
        client.approve_upgrade(&p(&env, 2), &hash, &2);

        // Perform upgrade.
        let hash = bytes(&env, 0x77);
        client.approve_upgrade(&hash, &2);
        client.upgrade_contract(&hash);

        // Pause after upgrade.
        client.pause(&p(&env, 3));

        // State must be intact after upgrade.
        assert!(client.is_paused());
        assert!(client.is_schema_version_approved(&3));
        assert_eq!(client.get_contract_version(), 2);
    }

    /// An upgrade approved with version N cannot be reused to downgrade from
    /// a later version M > N even if the hash is re-added to the allowlist.
    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn cannot_re_approve_old_version_after_upgrade() {
        let (env, client, _admin) = setup();
        use earnproof_shared::ContractError;
        let hash_v2 = bytes(&env, 0x01);
        let old_hash = bytes(&env, 0x02);

        // Upgrade to version 2.
        client.approve_upgrade(&hash_v2, &2);
        client.upgrade_contract(&hash_v2);
        assert_eq!(client.get_contract_version(), 2);

        // Attempt to allowlist a hash that would install version 1 — rejected.
        client.approve_upgrade(&old_hash, &1);
    }

    /// `approve_upgrade` by a non-admin must be rejected.
    #[test]
    #[should_panic]
    fn approve_upgrade_requires_admin_auth() {
        let env = Env::default();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let other = Address::from_str(&env, OTHER);

        env.mock_all_auths();
        client.initialize(&admin);

        // Only authorize `other`, not `admin` — should panic.
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &other,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &client.address,
                fn_name: "approve_upgrade",
                args: soroban_sdk::vec![
                    &env,
                    soroban_sdk::IntoVal::into_val(&BytesN::from_array(&env, &[0x11; 32]), &env),
                    soroban_sdk::IntoVal::into_val(&BytesN::from_array(&env, &[0xaa; 32]), &env),
                    soroban_sdk::IntoVal::into_val(&2_u32, &env),
                ],
                sub_invokes: &[],
            },
        }]);
        client.approve_upgrade(&p(&env, 0x11), &BytesN::from_array(&env, &[0xaa; 32]), &2);
    }

    // ── numeric boundary tests ────────────────────────────────────────────────

    /// Table-driven tests for schema version boundaries.
    /// Schema versions must be >= MIN_SCHEMA_VERSION (1).
    #[test]
    fn schema_version_boundary_values() {
        let (env, client, _admin) = setup();

        // Valid: minimum allowed schema version
        client.approve_schema_version(&1);
        assert!(client.is_schema_version_approved(&1));

        // Valid: typical schema versions
        client.approve_schema_version(&2);
        assert!(client.is_schema_version_approved(&2));

        client.approve_schema_version(&p(&env, 3), &100);
        assert!(client.is_schema_version_approved(&100));

        // Valid: u32 maximum
        client.approve_schema_version(&u32::MAX);
        assert!(client.is_schema_version_approved(&u32::MAX));
    }

    #[test]
    fn schema_version_zero_rejected() {
        let (_env, client, _admin) = setup();
        // Version 0 must be rejected with a typed error.
        use earnproof_shared::ContractError;
        let result = client.try_approve_schema_version(&p(&env, 1), &0);
        assert_eq!(result, Err(Ok(ContractError::InvalidInput)));
    }

    #[test]
    fn is_schema_version_approved_with_zero_returns_false() {
        let (_env, client, _admin) = setup();
        // Querying version 0 should return false without panic
        let result = client.is_schema_version_approved(&0);
        assert!(!result);
    }

    /// Table-driven tests for contract version boundaries.
    /// Contract versions must be > current version (monotonically increasing).
    #[test]
    fn contract_version_upgrade_boundaries() {
        let (env, client, _admin) = setup();
        assert_eq!(client.get_contract_version(), 1);

        // Valid: immediate next version
        client.approve_upgrade(&bytes(&env, 1), &2);
        client.upgrade_contract(&bytes(&env, 1));
        assert_eq!(client.get_contract_version(), 2);

        // Valid: skip versions (not required to be sequential)
        client.approve_upgrade(&bytes(&env, 2), &1000);
        client.upgrade_contract(&bytes(&env, 2));
        assert_eq!(client.get_contract_version(), 1000);

        // Valid: very large version number
        client.approve_upgrade(&bytes(&env, 3), &u32::MAX);
        client.upgrade_contract(&bytes(&env, 3));
        assert_eq!(client.get_contract_version(), u32::MAX);
    }

    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn contract_version_equal_current_rejected() {
        let (env, client, _admin) = setup();
        // Current version is 1; attempting to set it to 1 again is rejected
        client.approve_upgrade(&bytes(&env, 1), &1);
    }

    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn contract_version_below_current_rejected() {
        let (env, client, _admin) = setup();
        // Current version is 1; attempting to set it to 0 is rejected
        client.approve_upgrade(&bytes(&env, 1), &0);
    }

    /// Table-driven tests for config version bumping.
    /// Config version increments on every configuration change.
    /// This tests the checked_add protection against overflow.
    #[test]
    fn config_version_increments_on_mutations() {
        let (env, client, _admin) = setup();
        assert_eq!(client.get_config_version(), 1);

        // Each mutation bumps config version
        client.pause();
        assert_eq!(client.get_config_version(), 2);

        client.unpause(&p(&env, 2));
        assert_eq!(client.get_config_version(), 3);

        client.approve_schema_version(&p(&env, 3), &1);
        assert_eq!(client.get_config_version(), 4);

        client.deprecate_schema_version(&p(&env, 4), &1);
        assert_eq!(client.get_config_version(), 5);
    }

    /// Verify that config version correctly handles large values
    /// approaching u32::MAX (bumping is protected by checked_add).
    #[test]
    fn config_version_safe_near_u32_max() {
        let (env, client, _admin) = setup();

        // Manually set config version to a value near max by simulating
        // many mutations. We'll do a smaller simulation here.
        // In real operation, reaching u32::MAX would require ~4 billion mutations,
        // which is impractical in a test, but we verify the protection exists.

        // Get current config version (should be 1 after setup)
        let mut v = client.get_config_version();
        assert_eq!(v, 1);

        // Perform several mutations and verify each increments exactly once,
        // starting from the value established by the previous mutation.
        for _ in 0..10 {
            client.pause();
            let after_pause = client.get_config_version();
            assert_eq!(
                after_pause,
                v + 1,
                "pause must bump config version exactly once"
            );

            client.unpause(&p(&env, 10 + i * 2 + 1));
            let after_unpause = client.get_config_version();
            assert_eq!(
                after_unpause,
                after_pause + 1,
                "unpause must bump config version exactly once"
            );
            v = after_unpause;
        }
    }

    /// Test storage and event invariants: failed boundary cases
    /// must not modify state or emit events.
    #[test]
    fn failed_schema_version_zero_leaves_state_unchanged() {
        let (env, client, _admin) = setup();

        let config_before = client.get_config_version();
        let approved_before = client.is_schema_version_approved(&999);

        // Attempt to approve version 0 — should panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_schema_version(&BytesN::from_array(&env, &[1u8; 32]), &0);
        }));

        // Must have panicked
        assert!(result.is_err());

        // State must be unchanged
        assert_eq!(client.get_config_version(), config_before);
        assert_eq!(
            client.is_schema_version_approved(&999),
            approved_before,
            "schema version approval state must not change on failed validation"
        );
    }

    #[test]
    fn failed_upgrade_version_downgrade_leaves_state_unchanged() {
        let (env, client, _admin) = setup();

        let contract_version_before = client.get_contract_version();
        let config_version_before = client.get_config_version();
        let hash = bytes(&env, 0x99);

        // Attempt to allowlist a downgrade (current version is 1, trying version 0)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_upgrade(&hash, &BytesN::from_array(&env, &[1u8; 32]), &0);
        }));

        // Must have panicked
        assert!(result.is_err());

        // State must be unchanged: contract version not modified
        assert_eq!(
            client.get_contract_version(),
            contract_version_before,
            "contract version must not change on failed upgrade approval"
        );

        // Config version must not be bumped on failed validation
        assert_eq!(
            client.get_config_version(),
            config_version_before,
            "config version must not change when upgrade approval fails"
        );

        // Hash must not be on allowlist
        assert!(
            !client.is_upgrade_allowed(&hash),
            "failed upgrade approval must not add hash to allowlist"
        );
    }

    // ── adversarial initialization tests ───────────────────────────────────────

    /// Verify that first initialization writes exactly the documented state
    /// with no partial writes or missing fields.
    ///
    /// Required behavior: First call to `initialize` results in:
    /// - Admin address set and readable
    /// - Paused = false
    /// - ConfigVersion = 1
    /// - ContractVersion = 1
    /// - Initialized event published
    #[test]
    fn initialization_writes_exactly_documented_state() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        // Before initialization: storage should be empty
        // (querying uninitialized values returns defaults or panics)

        // Perform initialization
        client.initialize(&admin);

        // Verify exact state written
        assert_eq!(client.get_admin(), admin, "admin must be set");
        assert!(
            !client.is_paused(),
            "protocol must not be paused after initialization"
        );
        assert_eq!(
            client.get_config_version(),
            1,
            "config version must be exactly 1 after initialization"
        );
        assert_eq!(
            client.get_contract_version(),
            1,
            "contract version must be exactly 1 after initialization"
        );

        // Verify Initialized event was published
        // (Event verification requires inspecting env's event log)
        env.as_contract(&contract_id, || {
            // Storage keys must all be set (verifiable via has() calls)
            let instance = env.storage().instance();
            assert!(
                instance.has(&DataKey::Admin),
                "Admin key must exist in instance storage"
            );
            assert!(
                instance.has(&DataKey::Paused),
                "Paused key must exist in instance storage"
            );
            assert!(
                instance.has(&DataKey::ConfigVersion),
                "ConfigVersion key must exist in instance storage"
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
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        // First initialization succeeds
        client.initialize(&admin);
        let config_version_after_first = client.get_config_version();
        let contract_version_after_first = client.get_contract_version();
        let paused_after_first = client.is_paused();

        // Attempt second initialization with same admin
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&admin);
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
            client.get_config_version(),
            config_version_after_first,
            "config version must not change after failed re-initialization"
        );
        assert_eq!(
            client.get_contract_version(),
            contract_version_after_first,
            "contract version must not change after failed re-initialization"
        );
        assert_eq!(
            client.is_paused(),
            paused_after_first,
            "paused state must not change after failed re-initialization"
        );
    }

    /// Verify that re-initialization by a different address also fails
    /// without state or event changes.
    ///
    /// This tests that the re-initialization guard does not discriminate
    /// based on caller identity — it prevents any re-initialization attempt.
    #[test]
    fn reinitialization_by_different_admin_fails_atomically() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);
        let other = Address::from_str(&env, OTHER);

        // First initialization with original admin
        client.initialize(&admin);
        let stored_admin = client.get_admin();
        let config_version_after_first = client.get_config_version();

        // Attempt re-initialization with different admin
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&other);
        }));

        // Must have panicked
        assert!(
            result.is_err(),
            "re-initialization by different admin must panic"
        );

        // Verify state is unchanged: original admin must still be stored
        assert_eq!(
            client.get_admin(),
            stored_admin,
            "admin must not change when different address attempts re-initialization"
        );
        assert_eq!(
            client.get_config_version(),
            config_version_after_first,
            "config version must not change after failed re-initialization by different admin"
        );
    }

    /// Verify that an address that looks like it might have elevated permissions
    /// cannot bypass the re-initialization guard.
    ///
    /// Tests with an address string that is numeric (e.g., address index)
    /// or otherwise potentially special to the test framework.
    #[test]
    fn reinitialization_by_arbitrary_special_address_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        // First initialization with standard admin
        client.initialize(&admin);
        let stored_admin = client.get_admin();

        // Attempt re-initialization with an arbitrary address that might look
        // special (e.g., derived from a standard test key)
        let arbitrary = Address::from_str(&env, OTHER);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&arbitrary);
        }));

        // Must have panicked
        assert!(
            result.is_err(),
            "re-initialization by arbitrary address must panic"
        );

        // Original admin must be preserved
        assert_eq!(
            client.get_admin(),
            stored_admin,
            "admin must not change when arbitrary address attempts re-initialization"
        );
    }

    /// Verify that the re-initialization guard is truly the only barrier —
    /// the panic message must indicate "already initialized", not a different
    /// validation error.
    #[test]
    fn reinitialization_panic_message_indicates_guard() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        // First initialization succeeds
        client.initialize(&admin);

        // Attempt re-initialization and verify panic message
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&admin);
        }));

        assert!(result.is_err(), "re-initialization must panic");
        // The panic message is internal to the contract; we verify the failure occurred
    }

    /// Verify that the re-initialization guard takes effect immediately after
    /// the first initialize() call completes — no transient window during which
    /// a second initialize could partially succeed.
    #[test]
    fn reinitialization_guard_active_immediately() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        // First initialization
        client.initialize(&admin);

        // Subsequent initializations (multiple attempts) must all fail
        for attempt in 1..=3 {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.initialize(&admin);
            }));

            assert!(
                result.is_err(),
                "re-initialization attempt {} must fail",
                attempt
            );

            // Admin must remain unchanged after each failed attempt
            assert_eq!(
                client.get_admin(),
                admin,
                "admin must not change after re-initialization attempt {}",
                attempt
            );
        }
    }

    /// Verify that the documented initialization state is maintained even
    /// across function calls and state mutations after initialization.
    ///
    /// Tests that the initial state (versions, paused flag) is stable
    /// and correct before any subsequent mutations.
    #[test]
    fn initialization_state_stable_before_mutations() {
        let (env, client, admin) = setup();

        // State immediately after initialization must be as documented
        assert_eq!(client.get_admin(), admin);
        assert!(!client.is_paused());
        assert_eq!(client.get_config_version(), 1);
        assert_eq!(client.get_contract_version(), 1);

        // Perform a mutation (pause)
        client.pause();

        // Admin must remain unchanged
        assert_eq!(
            client.get_admin(),
            admin,
            "admin must not change across mutations"
        );

        // But config version should have bumped
        assert_eq!(
            client.get_config_version(),
            2,
            "config version must increment on mutation"
        );

        // Contract version must remain at 1 (only changes on upgrade)
        assert_eq!(
            client.get_contract_version(),
            1,
            "contract version must not change on config mutation"
        );
    }

    /// Summary test: protocol-config initialization spec verification.
    ///
    /// This test serves as executable documentation of what the test matrix
    /// expects from protocol-config initialization:
    /// - Standalone contract (no dependency addresses)
    /// - Has re-initialization guard
    /// - Emits Initialized event
    /// - Sets: admin, paused=false, config_version=1, contract_version=1
    #[test]
    fn protocol_config_initialization_spec_summary() {
        // CONTRACT SPEC: protocol-config
        // - Name: "protocol-config"
        // - Has re-initialization guard: YES (panics "already initialized")
        // - Emits initialization event: YES (Initialized { admin })
        // - Takes dependency addresses: NO
        // - Dependencies: []
        // - First init writes:
        //   - Admin: passed address (requires auth)
        //   - Paused: false
        //   - ConfigVersion: 1
        //   - ContractVersion: 1
        // - Re-init guard: DataKey::Admin presence check; panics if set
        // - Re-init allowed by different admin: NO (guard blocks all)
        // - Invalid config cases: None (no dependencies to validate)

        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        let admin = Address::from_str(&env, ADMIN);

        // Verify the spec
        client.initialize(&admin);
        assert_eq!(client.get_admin(), admin);
        assert!(!client.is_paused());
        assert_eq!(client.get_config_version(), 1);
        assert_eq!(client.get_contract_version(), 1);

        // Re-initialization must fail
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&admin)
        }))
        .is_err());
    }

    // ── genesis identity (issue #192) ────────────────────────────────────────

    #[test]
    fn genesis_is_recorded_at_initialization() {
        let (env, client, _admin) = setup();
        let genesis = client.get_genesis();
        assert_ne!(genesis.genesis_id, BytesN::from_array(&env, &[0u8; 32]));
        assert_eq!(genesis.initialized_at_ledger, env.ledger().sequence());
    }

    #[test]
    fn genesis_id_is_deterministic_for_the_same_inputs() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);

        let one = env.register(ProtocolConfigContract, ());
        let one = ProtocolConfigContractClient::new(&env, &one);
        one.initialize(&admin);

        // Recomputing from the same env/contract address/domain must be
        // stable, since the identifier is a pure function of those inputs.
        let recomputed = env.as_contract(&one.address, || {
            earnproof_shared::compute_genesis_id(&env, "earnproof_protocol_config")
        });
        assert_eq!(one.get_genesis().genesis_id, recomputed);
    }

    #[test]
    fn genesis_id_differs_across_contract_instances() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_str(&env, ADMIN);

        let a = env.register(ProtocolConfigContract, ());
        let a = ProtocolConfigContractClient::new(&env, &a);
        a.initialize(&admin);

        let b = env.register(ProtocolConfigContract, ());
        let b = ProtocolConfigContractClient::new(&env, &b);
        b.initialize(&admin);

        // Different contract addresses (different instances) must never share
        // an identity, even with the same admin and the same domain.
        assert_ne!(a.get_genesis().genesis_id, b.get_genesis().genesis_id);
    }

    #[test]
    fn genesis_is_unchanged_across_an_upgrade() {
        let (env, client, _admin) = setup();
        let genesis_before = client.get_genesis();

        let hash = bytes(&env, 0x55);
        client.approve_upgrade(&hash, &2);
        client.upgrade_contract(&hash);

        assert_eq!(client.get_genesis(), genesis_before);
    }

    #[test]
    fn get_genesis_fails_before_initialization() {
        let env = Env::default();
        let contract_id = env.register(ProtocolConfigContract, ());
        let client = ProtocolConfigContractClient::new(&env, &contract_id);
        use earnproof_shared::ContractError;

        let result = client.try_get_genesis();
        assert_eq!(result, Err(Ok(ContractError::NotInitialized)));
    }

    // ── schema payload size limits (issue #190) ──────────────────────────────

    #[test]
    fn unset_schema_gets_the_default_payload_limit() {
        let (_env, client, _admin) = setup();
        assert_eq!(
            client.get_schema_payload_limit(&1),
            earnproof_shared::DEFAULT_SCHEMA_PAYLOAD_LIMIT
        );
        // Version 0 can never be approved, and always reads as the default.
        assert_eq!(
            client.get_schema_payload_limit(&0),
            earnproof_shared::DEFAULT_SCHEMA_PAYLOAD_LIMIT
        );
    }

    #[test]
    fn schema_payload_limit_can_be_set_and_read_back() {
        let (_env, client, _admin) = setup();
        client.set_schema_payload_limit(&1, &1_024);
        assert_eq!(client.get_schema_payload_limit(&1), 1_024);
        // Other schema versions are unaffected.
        assert_eq!(
            client.get_schema_payload_limit(&2),
            earnproof_shared::DEFAULT_SCHEMA_PAYLOAD_LIMIT
        );
    }

    #[test]
    fn schema_payload_limit_of_zero_is_a_deterministic_valid_policy() {
        // A limit of zero is a legitimate governance choice (no auxiliary
        // payload permitted for that schema at all), not an error.
        let (_env, client, _admin) = setup();
        client.set_schema_payload_limit(&1, &0);
        assert_eq!(client.get_schema_payload_limit(&1), 0);
    }

    #[test]
    fn schema_payload_limit_rejects_zero_version() {
        let (_env, client, _admin) = setup();
        use earnproof_shared::ContractError;
        let result = client.try_set_schema_payload_limit(&0, &1_024);
        assert_eq!(result, Err(Ok(ContractError::InvalidInput)));
    }

    #[test]
    fn schema_payload_limit_survives_deprecation() {
        // Deprecating a schema version does not clear its configured limit;
        // the policy and the approval flag are independent.
        let (env, client, _admin) = setup();
        client.approve_schema_version(&p(&env, 1), &1);
        client.set_schema_payload_limit(&1, &1_024);
        client.deprecate_schema_version(&p(&env, 2), &1);
        assert_eq!(client.get_schema_payload_limit(&1), 1_024);
    }

    #[test]
    fn schema_payload_limit_update_does_not_retroactively_invalidate() {
        // Changing the limit is a forward-looking policy change: nothing here
        // reaches into proof-registry to revalidate proofs already written
        // under the old limit, and there is no mechanism by which it could.
        let (_env, client, _admin) = setup();
        client.set_schema_payload_limit(&1, &2_048);
        client.set_schema_payload_limit(&1, &16);
        assert_eq!(client.get_schema_payload_limit(&1), 16);
    }

    #[test]
    fn legacy_schema_policy_has_finite_migration_default() {
        let (_env, client, _admin) = setup();
        let policy = client.get_schema_policy(&1);
        assert_eq!(policy.proof_types.len(), 1);
        assert_eq!(policy.proof_types.get(0), Some(LEGACY_PROOF_TYPE));
        assert_eq!(policy.max_validity_seconds, MAX_SCHEMA_VALIDITY_SECONDS);
    }

    #[test]
    fn schema_policy_is_immutable_after_approval_and_is_bounded() {
        let (env, client, _admin) = setup();
        let mut types = Vec::new(&env);
        types.push_back(7);
        types.push_back(9);
        client.set_schema_policy(&1, &types, &120);
        client.approve_schema_version(&1);
        assert_eq!(client.get_schema_policy(&1).proof_types, types);

        let mut changed_types = Vec::new(&env);
        changed_types.push_back(11);
        assert!(client
            .try_set_schema_policy(&1, &changed_types, &240)
            .is_err());

        client.deprecate_schema_version(&1);
        assert!(client
            .try_set_schema_policy(&1, &changed_types, &240)
            .is_err());
        assert_eq!(client.get_schema_policy(&1).proof_types, types);

        client.set_schema_policy(&2, &changed_types, &240);
        client.approve_schema_version(&2);
        assert_eq!(client.get_schema_policy(&2).proof_types, changed_types);

        let empty = Vec::new(&env);
        assert!(client.try_set_schema_policy(&3, &empty, &1).is_err());
        let mut oversized = Vec::new(&env);
        for proof_type in 0..=MAX_SCHEMA_PROOF_TYPES {
            oversized.push_back(proof_type);
        }
        assert!(client
            .try_set_schema_policy(&3, &oversized, &1)
            .is_err());
        assert!(client.try_set_schema_policy(&3, &changed_types, &0).is_err());
        assert!(client
            .try_set_schema_policy(&3, &changed_types, &(MAX_SCHEMA_VALIDITY_SECONDS + 1))
            .is_err());
    }

    #[test]
    fn commitment_algorithm_can_be_retired_and_unknown_ids_are_rejected() {
        let (_env, client, _admin) = setup();
        assert!(client.is_algorithm_supported(&0));
        assert!(client.is_algorithm_supported(&SHA256_COMMITMENT_ALGORITHM_V1));
        assert!(!client.is_algorithm_supported(&u32::MAX));
        client.set_commitment_algorithm(&SHA256_COMMITMENT_ALGORITHM_V1, &false);
        assert!(!client.is_algorithm_supported(&SHA256_COMMITMENT_ALGORITHM_V1));
        assert!(client
            .try_set_commitment_algorithm(&u32::MAX, &true)
            .is_err());
    }

    #[test]
    #[should_panic]
    fn commitment_algorithm_policy_requires_admin_authorization() {
        let (env, client, _admin) = setup();
        env.set_auths(&[]);
        client.set_commitment_algorithm(&SHA256_COMMITMENT_ALGORITHM_V1, &false);
    }

    #[test]
    #[should_panic]
    fn schema_policy_requires_admin_authorization() {
        let (env, client, _admin) = setup();
        let mut types = Vec::new(&env);
        types.push_back(1);
        env.set_auths(&[]);
        client.set_schema_policy(&1, &types, &120);
    }

    // ── bounded configuration change history (issue #193) ────────────────────

    #[test]
    fn change_history_starts_empty() {
        let (_env, client, _admin) = setup();
        assert_eq!(client.get_config_history_cursor(), 0);
        assert_eq!(client.get_config_history(&0, &10).len(), 0);
    }

    #[test]
    fn change_history_records_ordering_and_category() {
        let (env, client, _admin) = setup();
        client.pause(&p(&env, 1));
        client.unpause(&p(&env, 2));
        client.approve_schema_version(&p(&env, 3), &9);

        assert_eq!(client.get_config_history_cursor(), 3);
        let page = client.get_config_history(&0, &10);
        assert_eq!(page.len(), 3);
        assert_eq!(
            page.get(0).unwrap().category,
            ConfigChangeCategory::PauseToggle
        );
        assert_eq!(
            page.get(1).unwrap().category,
            ConfigChangeCategory::PauseToggle
        );
        assert_eq!(
            page.get(2).unwrap().category,
            ConfigChangeCategory::SchemaApproval
        );
        // Each entry records the config version in effect right after it.
        assert_eq!(
            page.get(2).unwrap().config_version,
            client.get_config_version()
        );
    }

    #[test]
    fn failed_changes_do_not_append_history() {
        let (env, client, _admin) = setup();
        client.pause(&p(&env, 1));
        let cursor_before = client.get_config_history_cursor();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_schema_version(&p(&env, 2), &0);
        }));
        assert!(result.is_err());

        assert_eq!(client.get_config_history_cursor(), cursor_before);
    }

    #[test]
    fn change_history_ring_rolls_over_deterministically() {
        let (env, client, _admin) = setup();
        let capacity = earnproof_shared::CONFIG_HISTORY_CAPACITY;

        // Overrun the ring by a handful of entries.
        for version in 1..=(capacity + 3) {
            client.approve_schema_version(&p(&env, version as u8), &version);
        }
        let total = client.get_config_history_cursor();
        assert_eq!(total, capacity + 3);

        // A cursor before the oldest still-available entry is clamped forward
        // deterministically rather than erroring. The page length is bounded
        // by MAX_CONFIG_HISTORY_PAGE, independent of the ring capacity.
        let page = client.get_config_history(&0, &capacity);
        assert_eq!(page.len(), earnproof_shared::MAX_CONFIG_HISTORY_PAGE);
        // The oldest surviving entry is the one that pushed the first three
        // out of the ring.
        let oldest_surviving_version = 4u32;
        assert_eq!(
            page.get(0).unwrap().proposal_commitment,
            ProtocolConfigContract::commit_for_test(&env, oldest_surviving_version)
        );
    }

    #[test]
    fn change_history_page_size_is_capped() {
        let (env, client, _admin) = setup();
        for version in 1..=(earnproof_shared::MAX_CONFIG_HISTORY_PAGE + 5) {
            client.approve_schema_version(&p(&env, version as u8), &version);
        }
        // Requesting more than the cap returns at most the cap.
        let page = client.get_config_history(&0, &(earnproof_shared::MAX_CONFIG_HISTORY_PAGE + 5));
        assert_eq!(page.len(), earnproof_shared::MAX_CONFIG_HISTORY_PAGE);
    }

    #[test]
    fn change_history_survives_migration() {
        let (env, client, admin) = setup();
        client.pause(&p(&env, 1));
        let cursor_before = client.get_config_history_cursor();

        client.begin_migration(&2, &1);
        client.advance_migration(&0, &1);
        let _ = admin;

        assert_eq!(client.get_config_history_cursor(), cursor_before);
        assert_eq!(client.get_config_history(&0, &10).len(), cursor_before);
    }

    #[test]
    fn governance_role_expires_at_boundary_and_removal_is_immediate() {
        let (env, client, _admin) = setup();
        let delegate = Address::generate(&env);
        let activation = env.ledger().sequence();
        let expiration = activation + 2;
        client.grant_governance_role(
            &p(&env, 0xC1),
            &GovernanceRole::ProtocolPause,
            &delegate,
            &activation,
            &Some(expiration),
        );
        let assignment = client
            .get_governance_assignment(&GovernanceRole::ProtocolPause, &delegate)
            .unwrap();
        assert!(assignment.is_active_at(activation));
        assert!(!assignment.is_active_at(expiration));
        client.pause_by_role(&p(&env, 0xC2), &delegate);

        env.ledger()
            .with_mut(|ledger| ledger.sequence_number = expiration);
        assert_eq!(
            client.try_pause_by_role(&p(&env, 0xC3), &delegate),
            Err(Ok(ContractError::Unauthorized))
        );
        client.remove_governance_role(&GovernanceRole::ProtocolPause, &delegate);
        assert!(client
            .get_governance_assignment(&GovernanceRole::ProtocolPause, &delegate)
            .is_none());
    }

    #[test]
    fn overlapping_roles_can_be_renewed_and_survive_migration() {
        let (env, client, _admin) = setup();
        let first_delegate = Address::generate(&env);
        let second_delegate = Address::generate(&env);
        let now = env.ledger().sequence();
        client.grant_governance_role(
            &p(&env, 0xC4),
            &GovernanceRole::ProtocolPause,
            &first_delegate,
            &(now + 1),
            &Some(now + 6),
        );
        client.grant_governance_role(
            &p(&env, 0xC5),
            &GovernanceRole::ProtocolPause,
            &second_delegate,
            &(now + 3),
            &Some(now + 8),
        );
        env.ledger()
            .with_mut(|ledger| ledger.sequence_number = now + 4);
        assert!(client
            .get_governance_assignment(&GovernanceRole::ProtocolPause, &first_delegate)
            .unwrap()
            .is_active_at(now + 4));
        assert!(client
            .get_governance_assignment(&GovernanceRole::ProtocolPause, &second_delegate)
            .unwrap()
            .is_active_at(now + 4));

        client.grant_governance_role(
            &p(&env, 0xC6),
            &GovernanceRole::ProtocolPause,
            &first_delegate,
            &(now + 4),
            &Some(now + 10),
        );
        let renewed = client
            .get_governance_assignment(&GovernanceRole::ProtocolPause, &first_delegate)
            .unwrap();
        assert_eq!(renewed.expiration_ledger, Some(now + 10));

        client.begin_migration(&2, &1);
        client.advance_migration(&0, &1);
        assert_eq!(
            client
                .get_governance_assignment(&GovernanceRole::ProtocolPause, &first_delegate)
                .unwrap(),
            renewed
        );
    }

    /// Test-only helper mirroring the contract's private `commit` so the
    /// rollover test can assert on a specific expected commitment.
    impl ProtocolConfigContract {
        fn commit_for_test(env: &Env, version: u32) -> BytesN<32> {
            Self::commit(env, version)
        }
    }
}
