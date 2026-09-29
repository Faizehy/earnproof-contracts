#![no_std]

#[allow(unused_imports)]
use earnproof_shared::{
    compute_proof_context_commitments, derive_contextual_proof_id, is_interface_compatible,
    optional_subject_pseudonym_commitment, ContractError, CriticalAction, CriticalActionPolicy,
    CriticalActionProposal, GenesisRecord, InterfaceVersion, MigrationStatus, PauseScope,
    ProofAssetIdentifier, ProofContextCommitments, ProofError, ProofPayloadRecord, ProofRecord,
    ProofRegistrationContext, ProofStatus, ProofValidity, TtlStatus, UpgradeApproval,
    UpgradeReceipt, CRITICAL_ACTION_APPROVAL_EXPIRY_LEDGERS, ISSUER_REGISTRY_INTERFACE_VERSION,
    MAX_CRITICAL_ACTION_SIGNERS, MAX_MIGRATION_BATCH, MIGRATION_STATUS_VERSION,
    PROTOCOL_CONFIG_INTERFACE_VERSION, TTL_EXTEND_TO_LEDGERS, TTL_THRESHOLD_LEDGERS,
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

#[contractclient(name = "ProtocolConfigContractClient")]
pub trait ProtocolConfigInterface {
    fn is_paused(env: Env) -> bool;
    fn is_scope_paused(env: Env, scope: PauseScope) -> bool;
    fn is_schema_version_approved(env: Env, version: u32) -> bool;
    fn get_schema_payload_limit(env: Env, version: u32) -> u32;
    fn interface_version(env: Env) -> InterfaceVersion;
}

#[contractclient(name = "IssuerRegistryContractClient")]
pub trait IssuerRegistryInterface {
    fn is_active_address(env: Env, issuer_address: Address) -> bool;
    fn interface_version(env: Env) -> InterfaceVersion;
}

#[contract]
pub struct ProofRegistryContract;

struct ProofRegistration {
    proof_id_hash: BytesN<32>,
    commitment_hash: BytesN<32>,
    issuer_address: Address,
    schema_version: u32,
    expires_at: u64,
    payload: Option<Bytes>,
    context: Option<ProofContextCommitments>,
    subject_pseudonym_commitment: Option<BytesN<32>>,
}

#[contracttype]
enum DataKey {
    MigrationStatus,
    Admin,
    IssuerRegistry,
    ProtocolConfig,
    Proof(BytesN<32>),
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
    ProofContext(BytesN<32>),
    ProofSubjectPseudonym(BytesN<32>),
    PendingAdmin,
    CriticalActionPolicy,
    CriticalActionProposal(BytesN<32>),
    CriticalActionNonce,
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

// ── upgrade events ────────────────────────────────────────────────────────────

/// Emitted when the admin adds a WASM hash to the upgrade allowlist.
#[contractevent]
pub struct UpgradeAllowlisted {
    pub wasm_hash: BytesN<32>,
    pub new_contract_version: u32,
    pub approved_by: Address,
}

/// Emitted when the admin removes a WASM hash from the allowlist without
/// applying it.
#[contractevent]
pub struct UpgradeRevoked {
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

#[contractevent]
pub struct CriticalActionProposed {
    pub proposal_id: BytesN<32>,
    pub proposer: Address,
}

#[contractevent]
pub struct CriticalActionApproved {
    pub proposal_id: BytesN<32>,
    pub signer: Address,
}

#[contractevent]
pub struct CriticalActionCancelled {
    pub proposal_id: BytesN<32>,
    pub cancelled_by: Address,
}

#[contractevent]
pub struct CriticalActionExecuted {
    pub proposal_id: BytesN<32>,
}

#[contractevent]
pub struct CriticalActionPolicySet {
    pub enabled: bool,
    pub threshold: u32,
    pub signer_count: u32,
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
    pub epoch: u32,
}

/// Emitted when a proof is registered with an auxiliary payload.
#[contractevent]
pub struct ProofRegisteredWithPayload {
    pub proof_id_hash: BytesN<32>,
    pub payload_len: u32,
    pub payload_hash: BytesN<32>,
    pub epoch: u32,
}

/// Emitted when a proof is revoked, by its issuer or by the admin.
#[contractevent]
pub struct ProofRevoked {
    pub proof_id_hash: BytesN<32>,
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
    pub fn get_genesis(env: Env) -> Result<GenesisRecord, ContractError> {
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
    pub fn get_proof_payload(
        env: Env,
        proof_id_hash: BytesN<32>,
    ) -> Result<ProofPayloadRecord, ProofError> {
        let key = DataKey::ProofPayloadMeta(proof_id_hash);
        let record = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ProofError::ProofNotFound)?;
        Self::extend_payload_key_ttl(env, &key);
        Ok(record)
    }

    pub fn nominate_successor(env: Env, successor: Address) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        Self::require_valid_issuer_address(&successor)?;
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

    pub fn activate_successor(env: Env) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        let admin = Self::get_admin(env.clone()).map_err(|_| ProofError::ProofNotFound)?;
        Self::require_auth(&admin);
        let successor: Address = env
            .storage()
            .instance()
            .get(&DataKey::Successor)
            .ok_or(ProofError::ProofNotFound)?;

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

    pub fn keepalive_instance(env: Env) -> bool {
        if !env.storage().instance().has(&DataKey::Admin) {
            return false;
        }
        Self::extend_instance_ttl(env);
        true
    }

    pub fn keepalive_proof(env: Env, proof_id_hash: BytesN<32>) -> bool {
        let context_id = proof_id_hash.clone();
        let key = DataKey::Proof(proof_id_hash);
        if env.storage().persistent().has(&key) {
            Self::extend_proof_key_ttl(env.clone(), &key);
            Self::extend_proof_sidecars_ttl(env, &context_id);
            true
        } else {
            false
        }
    }

    pub fn register_proof(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
    ) -> Result<(), ProofError> {
        Self::register_proof_inner(
            env,
            ProofRegistration {
                proof_id_hash,
                commitment_hash,
                issuer_address,
                schema_version,
                expires_at,
                payload: None,
                context: None,
                subject_pseudonym_commitment: None,
            },
        )
    }

    /// Register a proof with an opaque subject pseudonym commitment. An
    /// all-zero commitment explicitly means that no pseudonym is recorded.
    pub fn register_proof_with_pseudonym(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        subject_pseudonym_commitment: BytesN<32>,
    ) -> Result<(), ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        Self::require_valid_issuer_address(&issuer_address)?;
        Self::require_auth(&issuer_address);
        let subject_pseudonym_commitment =
            optional_subject_pseudonym_commitment(&subject_pseudonym_commitment);
        Self::register_proof_inner(
            env,
            ProofRegistration {
                proof_id_hash,
                commitment_hash,
                issuer_address,
                schema_version,
                expires_at,
                payload: None,
                context: None,
                subject_pseudonym_commitment,
            },
        )
    }

    pub fn register_proof_with_context(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        registration_context: ProofRegistrationContext,
    ) -> Result<BytesN<32>, ProofError> {
        Self::ensure_not_decommissioned(&env)?;
        Self::require_valid_issuer_address(&issuer_address)?;
        Self::require_auth(&issuer_address);
        let context = compute_proof_context_commitments(
            &env,
            &commitment_hash,
            &registration_context.network_passphrase,
            &registration_context.asset,
        )
        .ok_or(ProofError::InvalidProofContext)?;
        let contextual_proof_id =
            derive_contextual_proof_id(&env, &proof_id_hash, &context.proof_context_commitment);
        Self::register_proof_inner(
            env,
            ProofRegistration {
                proof_id_hash: contextual_proof_id.clone(),
                commitment_hash,
                issuer_address,
                schema_version,
                expires_at,
                payload: registration_context.payload,
                context: Some(context),
                subject_pseudonym_commitment: optional_subject_pseudonym_commitment(
                    &registration_context.subject_pseudonym_commitment,
                ),
            },
        )?;
        Ok(contextual_proof_id)
    }

    /// Registers a proof exactly like [`Self::register_proof`], plus an
    /// auxiliary payload whose size is bounded by the schema's governed
    /// limit (`ProtocolConfigContract::get_schema_payload_limit`). The bound
    /// is enforced before any state is written. Only the payload's length
    /// and a commitment hash are stored — never the raw bytes — so resource
    /// use stays bounded regardless of the configured limit.
    pub fn register_proof_with_payload(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        payload: Bytes,
    ) -> Result<(), ProofError> {
        Self::register_proof_inner(
            env,
            ProofRegistration {
                proof_id_hash,
                commitment_hash,
                issuer_address,
                schema_version,
                expires_at,
                payload: Some(payload),
                context: None,
                subject_pseudonym_commitment: None,
            },
        )
    }

    pub fn get_proof_context_commitments(
        env: Env,
        proof_id_hash: BytesN<32>,
    ) -> Result<Option<ProofContextCommitments>, ProofError> {
        Self::get_proof(env.clone(), proof_id_hash.clone())?;
        let key = DataKey::ProofContext(proof_id_hash);
        Ok(env.storage().persistent().get(&key))
    }

    pub fn get_proof_pseudonym_commitment(
        env: Env,
        proof_id_hash: BytesN<32>,
    ) -> Result<Option<BytesN<32>>, ProofError> {
        Self::get_proof(env.clone(), proof_id_hash.clone())?;
        let key = DataKey::ProofSubjectPseudonym(proof_id_hash);
        Ok(env.storage().persistent().get(&key))
    }

    fn register_proof_inner(env: Env, registration: ProofRegistration) -> Result<(), ProofError> {
        let ProofRegistration {
            proof_id_hash,
            commitment_hash,
            issuer_address,
            schema_version,
            expires_at,
            payload,
            context,
            subject_pseudonym_commitment,
        } = registration;
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

        if expires_at <= env.ledger().timestamp() {
            return Err(ProofError::ProofExpired);
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

        let payload_len = if let Some(payload) = &payload {
            let max_payload_size = protocol_client.get_schema_payload_limit(&schema_version);
            let length = payload.len();
            if length > max_payload_size {
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

        let now = env.ledger().timestamp();
        let created_ledger = env.ledger().sequence();
        let record = ProofRecord {
            proof_id_hash: proof_id_hash.clone(),
            commitment_hash,
            issuer_address,
            status: ProofStatus::Active,
            schema_version,
            expires_at,
            created_at: now,
            revoked_at: 0,
            created_ledger,
        };
        env.storage().persistent().set(&key, &record);
        Self::extend_proof_key_ttl(env.clone(), &key);

        if let Some(context) = context {
            let context_key = DataKey::ProofContext(proof_id_hash.clone());
            env.storage().persistent().set(&context_key, &context);
            Self::extend_proof_key_ttl(env.clone(), &context_key);
        }

        if let Some(commitment) = subject_pseudonym_commitment {
            let pseudonym_key = DataKey::ProofSubjectPseudonym(proof_id_hash.clone());
            env.storage().persistent().set(&pseudonym_key, &commitment);
            Self::extend_proof_key_ttl(env.clone(), &pseudonym_key);
        }

        if let (Some(payload), Some(payload_len)) = (payload, payload_len) {
            let payload_hash = env.crypto().sha256(&payload).to_bytes();
            let payload_key = DataKey::ProofPayloadMeta(proof_id_hash.clone());
            let payload_meta = ProofPayloadRecord {
                payload_len,
                payload_hash: payload_hash.clone(),
            };
            env.storage().persistent().set(&payload_key, &payload_meta);
            Self::extend_payload_key_ttl(env.clone(), &payload_key);

            let epoch = Self::bump_registry_epoch(&env);
            ProofRegisteredWithPayload {
                proof_id_hash,
                payload_len,
                payload_hash,
                epoch,
            }
            .publish(&env);
        } else {
            let epoch = Self::bump_registry_epoch(&env);
            ProofRegistered {
                proof_id_hash,
                epoch,
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

    pub fn get_proof(env: Env, proof_id_hash: BytesN<32>) -> Result<ProofRecord, ProofError> {
        let context_id = proof_id_hash.clone();
        let key = DataKey::Proof(proof_id_hash);
        let record = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ProofError::ProofNotFound)?;
        Self::extend_proof_key_ttl(env.clone(), &key);
        Self::extend_proof_sidecars_ttl(env, &context_id);
        Ok(record)
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

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)
    }

    pub fn get_issuer_registry(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .instance()
            .get(&DataKey::IssuerRegistry)
            .ok_or(ContractError::NotInitialized)
    }

    pub fn get_protocol_config(env: Env) -> Result<Address, ContractError> {
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
    pub fn bound_issuer_registry_version(env: Env) -> Result<InterfaceVersion, ContractError> {
        let address = Self::get_issuer_registry(env.clone())?;
        Ok(IssuerRegistryContractClient::new(&env, &address).interface_version())
    }

    /// Live interface version currently reported by the bound protocol config.
    pub fn bound_protocol_config_version(env: Env) -> Result<InterfaceVersion, ContractError> {
        let address = Self::get_protocol_config(env.clone())?;
        Ok(ProtocolConfigContractClient::new(&env, &address).interface_version())
    }

    /// Admin-only: replace the bound issuer registry.
    ///
    /// The replacement is validated as a distinct, well-formed principal and
    /// must pass the interface handshake before it is stored. The check is not
    /// gated by any paused state and does not run through the upgrade flow, so
    /// it cannot be bypassed.
    pub fn set_issuer_registry(
        env: Env,
        new_issuer_registry: Address,
    ) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        Self::ensure_critical_action_policy_disabled(&env)?;

        let protocol_config = Self::get_protocol_config(env.clone())?;
        Self::validate_dependency_addresses(&env, &new_issuer_registry, &protocol_config)?;
        let actual =
            IssuerRegistryContractClient::new(&env, &new_issuer_registry).interface_version();
        if !is_interface_compatible(&REQUIRED_ISSUER_REGISTRY_VERSION, &actual) {
            return Err(ContractError::IncompatibleInterfaceVersion);
        }

        env.storage()
            .instance()
            .set(&DataKey::IssuerRegistry, &new_issuer_registry);
        Self::extend_instance_ttl(env);
        Ok(())
    }

    /// Admin-only: replace the bound protocol config. Same guarantees as
    /// `set_issuer_registry`.
    pub fn set_protocol_config(
        env: Env,
        new_protocol_config: Address,
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

        env.storage()
            .instance()
            .set(&DataKey::ProtocolConfig, &new_protocol_config);
        Self::extend_instance_ttl(env);
        Ok(())
    }

    pub fn get_critical_action_policy(env: Env) -> CriticalActionPolicy {
        Self::critical_action_policy(&env)
    }

    /// Bootstrap or replace the critical-action policy. Once enabled, policy
    /// updates must themselves receive threshold approval.
    pub fn set_critical_action_policy(
        env: Env,
        policy: CriticalActionPolicy,
    ) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        if Self::critical_action_policy(&env).enabled {
            return Err(ContractError::ThresholdApprovalRequired);
        }
        Self::validate_critical_action_policy(&policy)?;
        Self::store_critical_action_policy(env, policy);
        Ok(())
    }

    pub fn propose_critical_action(
        env: Env,
        action: CriticalAction,
    ) -> Result<BytesN<32>, ContractError> {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        let policy = Self::critical_action_policy(&env);
        if !policy.enabled {
            return Err(ContractError::InvalidState);
        }
        if !matches!(
            &action,
            CriticalAction::IssuerRegistryReplacement(_)
                | CriticalAction::ProtocolConfigReplacement(_)
                | CriticalAction::ApprovalPolicyUpdate(_)
        ) {
            return Err(ContractError::InvalidInput);
        }

        let nonce = env
            .storage()
            .instance()
            .get::<_, u32>(&DataKey::CriticalActionNonce)
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(ContractError::InvalidState)?;
        let proposal_id = Self::critical_action_commitment(&env, &action, nonce);
        let created_at = env.ledger().sequence();
        let expires_at = created_at
            .checked_add(CRITICAL_ACTION_APPROVAL_EXPIRY_LEDGERS)
            .ok_or(ContractError::InvalidInput)?;
        env.storage()
            .instance()
            .set(&DataKey::CriticalActionNonce, &nonce);
        let proposal = CriticalActionProposal {
            category: action.category(),
            action,
            policy,
            proposer: admin.clone(),
            approvals: Vec::new(&env),
            created_at,
            expires_at,
        };
        let key = DataKey::CriticalActionProposal(proposal_id.clone());
        env.storage().persistent().set(&key, &proposal);
        env.storage().persistent().extend_ttl(
            &key,
            TTL_THRESHOLD_LEDGERS,
            CRITICAL_ACTION_APPROVAL_EXPIRY_LEDGERS + TTL_THRESHOLD_LEDGERS,
        );
        Self::extend_instance_ttl(env.clone());
        CriticalActionProposed {
            proposal_id: proposal_id.clone(),
            proposer: admin,
        }
        .publish(&env);
        Ok(proposal_id)
    }

    pub fn get_critical_action_proposal(
        env: Env,
        proposal_id: BytesN<32>,
    ) -> Option<CriticalActionProposal> {
        let key = DataKey::CriticalActionProposal(proposal_id);
        let proposal = env.storage().persistent().get(&key);
        if proposal.is_some() {
            env.storage().persistent().extend_ttl(
                &key,
                TTL_THRESHOLD_LEDGERS,
                CRITICAL_ACTION_APPROVAL_EXPIRY_LEDGERS + TTL_THRESHOLD_LEDGERS,
            );
        }
        proposal
    }

    pub fn approve_critical_action(
        env: Env,
        proposal_id: BytesN<32>,
        signer: Address,
    ) -> Result<(), ContractError> {
        let mut proposal = Self::get_critical_action_proposal(env.clone(), proposal_id.clone())
            .ok_or(ContractError::ApprovalProposalNotFound)?;
        Self::ensure_proposal_live(&env, &proposal)?;
        Self::require_auth(&signer);
        if !proposal.policy.signers.contains(&signer) {
            return Err(ContractError::Unauthorized);
        }
        if proposal.approvals.contains(&signer) {
            return Err(ContractError::AlreadyExists);
        }
        proposal.approvals.push_back(signer.clone());
        env.storage().persistent().set(
            &DataKey::CriticalActionProposal(proposal_id.clone()),
            &proposal,
        );
        CriticalActionApproved {
            proposal_id,
            signer,
        }
        .publish(&env);
        Ok(())
    }

    pub fn cancel_critical_action(env: Env, proposal_id: BytesN<32>) -> Result<(), ContractError> {
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        let key = DataKey::CriticalActionProposal(proposal_id.clone());
        if !env.storage().persistent().has(&key) {
            return Err(ContractError::ApprovalProposalNotFound);
        }
        env.storage().persistent().remove(&key);
        CriticalActionCancelled {
            proposal_id,
            cancelled_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn execute_critical_action(env: Env, proposal_id: BytesN<32>) -> Result<(), ContractError> {
        let proposal = Self::get_critical_action_proposal(env.clone(), proposal_id.clone())
            .ok_or(ContractError::ApprovalProposalNotFound)?;
        Self::ensure_proposal_live(&env, &proposal)?;
        if proposal.approvals.len() < proposal.policy.threshold {
            return Err(ContractError::InsufficientApprovals);
        }
        match proposal.action {
            CriticalAction::IssuerRegistryReplacement(new_issuer_registry) => {
                let protocol_config = Self::get_protocol_config(env.clone())?;
                Self::validate_dependency_addresses(&env, &new_issuer_registry, &protocol_config)?;
                let actual = IssuerRegistryContractClient::new(&env, &new_issuer_registry)
                    .interface_version();
                if !is_interface_compatible(&REQUIRED_ISSUER_REGISTRY_VERSION, &actual) {
                    return Err(ContractError::IncompatibleInterfaceVersion);
                }
                env.storage()
                    .instance()
                    .set(&DataKey::IssuerRegistry, &new_issuer_registry);
                Self::extend_instance_ttl(env.clone());
            }
            CriticalAction::ProtocolConfigReplacement(new_protocol_config) => {
                let issuer_registry = Self::get_issuer_registry(env.clone())?;
                Self::validate_dependency_addresses(&env, &issuer_registry, &new_protocol_config)?;
                let actual = ProtocolConfigContractClient::new(&env, &new_protocol_config)
                    .interface_version();
                if !is_interface_compatible(&REQUIRED_PROTOCOL_CONFIG_VERSION, &actual) {
                    return Err(ContractError::IncompatibleInterfaceVersion);
                }
                env.storage()
                    .instance()
                    .set(&DataKey::ProtocolConfig, &new_protocol_config);
                Self::extend_instance_ttl(env.clone());
            }
            CriticalAction::ApprovalPolicyUpdate(policy) => {
                Self::validate_critical_action_policy(&policy)?;
                Self::store_critical_action_policy(env.clone(), policy);
            }
            _ => return Err(ContractError::InvalidInput),
        }
        env.storage()
            .persistent()
            .remove(&DataKey::CriticalActionProposal(proposal_id.clone()));
        CriticalActionExecuted { proposal_id }.publish(&env);
        Ok(())
    }

    // ── upgrade governance ────────────────────────────────────────────────────

    /// Returns the stored monotonic contract version.  Starts at 1.
    pub fn get_contract_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0)
    }

    /// Admin-only: add `wasm_hash` to the upgrade allowlist.
    ///
    /// `new_version` must be strictly greater than the current contract
    /// version to prevent pre-approving a downgrade.
    pub fn approve_upgrade(env: Env, wasm_hash: BytesN<32>, new_version: u32) {
        if Self::is_decommissioned(env.clone()) {
            panic!("contract is decommissioned");
        }
        if let Ok(protocol_config) = Self::get_protocol_config(env.clone()) {
            let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_config);
            if protocol_client.is_scope_paused(&PauseScope::Upgrades) {
                panic!("upgrades are paused");
            }
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

    /// Admin-only: remove a hash from the allowlist without applying it.
    pub fn revoke_upgrade(env: Env, wasm_hash: BytesN<32>) {
        if Self::is_decommissioned(env.clone()) {
            panic!("contract is decommissioned");
        }
        let admin = Self::get_admin(env.clone()).expect("contract not initialized");
        Self::require_auth(&admin);

        env.storage()
            .instance()
            .remove(&DataKey::AllowedWasm(wasm_hash.clone()));

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
    /// Requirements:
    /// 1. Caller is the admin.
    /// 2. `wasm_hash` is on the allowlist.
    /// 3. Target version is strictly greater than current (downgrade guard).
    ///
    /// On success the allowlist entry is consumed and `ContractVersion` is
    /// advanced.
    pub fn upgrade_contract(env: Env, wasm_hash: BytesN<32>) {
        if Self::is_decommissioned(env.clone()) {
            panic!("contract is decommissioned");
        }
        if let Ok(protocol_config) = Self::get_protocol_config(env.clone()) {
            let protocol_client = ProtocolConfigContractClient::new(&env, &protocol_config);
            if protocol_client.is_scope_paused(&PauseScope::Upgrades) {
                panic!("upgrades are paused");
            }
        }
        let admin = Self::get_admin(env.clone()).expect("contract not initialized");
        Self::require_auth(&admin);

        let new_version: u32 = env
            .storage()
            .instance()
            .get(&DataKey::AllowedWasm(wasm_hash.clone()))
            .expect("wasm hash not on allowlist");

        let old_version = Self::get_contract_version(env.clone());
        if new_version <= old_version {
            panic!("upgrade would not advance contract version");
        }

        // Consume allowlist entry before applying to prevent replay.
        env.storage()
            .instance()
            .remove(&DataKey::AllowedWasm(wasm_hash.clone()));

        #[cfg(not(test))]
        env.deployer()
            .update_current_contract_wasm(wasm_hash.clone());

        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &new_version);
        Self::extend_instance_ttl(env.clone());

        ContractUpgraded {
            new_wasm_hash: wasm_hash,
            old_contract_version: old_version,
            new_contract_version: new_version,
            upgraded_by: admin,
        }
        .publish(&env);
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

    fn require_valid_principal(address: &Address) -> Result<(), ContractError> {
        if !earnproof_shared::is_valid_principal_address(address) {
            return Err(ContractError::InvalidInput);
        }
        Ok(())
    }

    /// Queries each dependency's interface version and rejects any that is not
    /// compatible with the range this contract requires.
    fn require_compatible_dependencies(
        env: &Env,
        issuer_registry: &Address,
        protocol_config: &Address,
    ) -> Result<(), ContractError> {
        let issuer_version =
            IssuerRegistryContractClient::new(env, issuer_registry).interface_version();
        if !is_interface_compatible(&REQUIRED_ISSUER_REGISTRY_VERSION, &issuer_version) {
            return Err(ContractError::IncompatibleInterfaceVersion);
        }
        let config_version =
            ProtocolConfigContractClient::new(env, protocol_config).interface_version();
        if !is_interface_compatible(&REQUIRED_PROTOCOL_CONFIG_VERSION, &config_version) {
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
        env.storage().persistent().set(&key, &record);
        Self::extend_proof_key_ttl(env.clone(), &key);
        Self::extend_proof_sidecars_ttl(env.clone(), &proof_id_hash);
        let epoch = Self::bump_registry_epoch(&env);
        ProofRevoked {
            proof_id_hash,
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
    }

    fn extend_proof_key_ttl(env: Env, key: &DataKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
    }

    fn extend_proof_sidecars_ttl(env: Env, proof_id_hash: &BytesN<32>) {
        for key in [
            DataKey::ProofContext(proof_id_hash.clone()),
            DataKey::ProofSubjectPseudonym(proof_id_hash.clone()),
        ] {
            if env.storage().persistent().has(&key) {
                Self::extend_proof_key_ttl(env.clone(), &key);
            }
        }
    }

    fn extend_payload_key_ttl(env: Env, key: &DataKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
    }

    /// Advances the registry epoch by exactly one and returns the new value.
    /// Only ever called from a success path, after the mutation it
    /// summarizes has already been committed, so a failed call or a
    /// cross-contract rejection never advances it.
    ///
    /// Deliberately does not call `extend_instance_ttl`: proof-registry's
    /// documented TTL policy is that only `initialize` extends the instance
    /// entry, so per-call restoration cost stays flat regardless of traffic.
    /// See `docs/storage-ttl.md`.
    fn bump_registry_epoch(env: &Env) -> u32 {
        let current = Self::get_registry_epoch(env.clone());
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| panic!("registry epoch overflow: reached maximum"));
        env.storage().instance().set(&DataKey::RegistryEpoch, &next);
        next
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
    use earnproof_shared::{
        ContractError, CriticalAction, CriticalActionPolicy, ProofError, ProofStatus,
        CRITICAL_ACTION_APPROVAL_EXPIRY_LEDGERS, TTL_THRESHOLD_LEDGERS,
    };
    use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
    use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
    use soroban_sdk::{
        testutils::{storage::Persistent as _, Address as _, Ledger as _},
        Address, BytesN, Env, Vec,
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

        client.register_proof(&proof_id, &commitment, &issuer, &1, &2_000);

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

        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
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

        let result = client.try_register_proof(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &Address::from_str(&env, ISSUER),
            &1,
            &0,
        );
        assert_eq!(result, Err(Ok(ProofError::ProofExpired)));
    }

    #[test]
    fn rejects_duplicate_proof_id() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        use earnproof_shared::ProofError;
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);

        let result = client.try_register_proof(&proof_id, &bytes(&env, 3), &issuer, &1, &2_000);
        assert_eq!(result, Err(Ok(ProofError::ProofAlreadyRegistered)));
    }

    #[test]
    fn rejects_unapproved_schema_version() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        use earnproof_shared::ProofError;

        let result = client.try_register_proof(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &Address::from_str(&env, ISSUER),
            &2,
            &2_000,
        );
        assert_eq!(result, Err(Ok(ProofError::UnsupportedSchema)));
    }

    #[test]
    fn rejects_registration_when_protocol_is_paused() {
        let (env, client, protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        use earnproof_shared::ProofError;
        protocol_config.pause();

        let result = client.try_register_proof(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &Address::from_str(&env, ISSUER),
            &1,
            &2_000,
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
            &bytes(&env, 10),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        );

        let result = client.try_register_proof(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &inactive_issuer,
            &1,
            &2_000,
        );
        assert_eq!(result, Err(Ok(ProofError::IssuerInactive)));
    }

    #[test]
    fn extends_proof_storage_ttl() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);

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
        client.approve_upgrade(&hash, &2);
        assert!(client.is_upgrade_allowed(&hash));
    }

    #[test]
    fn revoke_removes_from_allowlist() {
        let (env, client, ..) = setup();
        let hash = bytes(&env, 0xcd);

        client.approve_upgrade(&hash, &2);
        client.revoke_upgrade(&hash);
        assert!(!client.is_upgrade_allowed(&hash));
    }

    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn approve_upgrade_rejects_downgrade_version() {
        let (env, client, ..) = setup();
        client.approve_upgrade(&bytes(&env, 1), &1);
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
        pc_client.approve_schema_version(&1);
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

        client.approve_upgrade(&hash, &2);
        client.upgrade_contract(&hash);

        assert_eq!(client.get_contract_version(), 2);
        assert!(!client.is_upgrade_allowed(&hash));
    }

    #[test]
    #[should_panic(expected = "wasm hash not on allowlist")]
    fn upgrade_hash_cannot_be_replayed() {
        let (env, client, ..) = setup();
        let hash = bytes(&env, 0x42);

        client.approve_upgrade(&hash, &2);
        client.upgrade_contract(&hash);
        client.upgrade_contract(&hash);
    }

    /// Persistent proof state must survive an upgrade.
    #[test]
    fn state_preserved_across_upgrade() {
        let (env, client, ..) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);

        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
        assert!(client.is_valid_proof(&proof_id));

        let hash = bytes(&env, 0x77);
        client.approve_upgrade(&hash, &2);
        client.upgrade_contract(&hash);

        assert!(client.is_valid_proof(&proof_id));
        assert_eq!(client.get_contract_version(), 2);
    }

    #[test]
    #[should_panic(expected = "new_version must be greater than current contract version")]
    fn cannot_re_approve_old_version_after_upgrade() {
        let (env, client, ..) = setup();
        let hash_v2 = bytes(&env, 0x01);
        let old_hash = bytes(&env, 0x02);

        client.approve_upgrade(&hash_v2, &2);
        client.upgrade_contract(&hash_v2);

        client.approve_upgrade(&old_hash, &1);
    }

    // ── numeric boundary tests ────────────────────────────────────────────────

    /// Table-driven tests for schema version boundaries in proof registration.
    /// Schema versions must be >= MIN_SCHEMA_VERSION (1).
    #[test]
    fn register_proof_schema_version_boundaries() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);

        // Valid: minimum allowed schema version
        client.register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &2_000);
        assert!(client.is_valid_proof(&bytes(&env, 1)));

        // Valid: typical schema version
        _pc.approve_schema_version(&99);
        client.register_proof(&bytes(&env, 10), &bytes(&env, 11), &issuer, &99, &2_000);
        assert!(client.is_valid_proof(&bytes(&env, 10)));

        // Valid: large schema version
        _pc.approve_schema_version(&u32::MAX);
        client.register_proof(
            &bytes(&env, 20),
            &bytes(&env, 21),
            &issuer,
            &u32::MAX,
            &2_000,
        );
        assert!(client.is_valid_proof(&bytes(&env, 20)));
    }

    #[test]
    fn register_proof_schema_version_zero_rejected() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);

        // Schema version 0 must be rejected with a typed error.
        let result =
            client.try_register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &0, &2_000);
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
        client.register_proof(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &(current_time + 1),
        );
        assert!(client.is_valid_proof(&bytes(&env, 1)));

        // Valid: reasonable future expiration (1 year in seconds)
        client.register_proof(
            &bytes(&env, 10),
            &bytes(&env, 11),
            &issuer,
            &1,
            &(current_time + 365 * 24 * 3600),
        );
        assert!(client.is_valid_proof(&bytes(&env, 10)));

        // Valid: far future (max u64 is reachable in practice)
        client.register_proof(&bytes(&env, 20), &bytes(&env, 21), &issuer, &1, &u64::MAX);
        assert!(client.is_valid_proof(&bytes(&env, 20)));
    }

    #[test]
    fn register_proof_expiration_at_current_time_rejected() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let current_time = env.ledger().timestamp();

        // Expiration equal to current time is rejected with a typed error.
        let result =
            client.try_register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &current_time);
        assert_eq!(result, Err(Ok(ProofError::ProofExpired)));
    }

    #[test]
    fn register_proof_expiration_in_past_rejected() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let current_time = env.ledger().timestamp();

        // Expiration in the past is rejected with a typed error.
        if current_time > 0 {
            let result = client.try_register_proof(
                &bytes(&env, 1),
                &bytes(&env, 2),
                &issuer,
                &1,
                &(current_time - 1),
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
            client.register_proof(&proof_id, &bytes(&env, 88), &issuer, &0, &2_000);
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
        client.approve_upgrade(&bytes(&env, 1), &2);
        client.upgrade_contract(&bytes(&env, 1));
        assert_eq!(client.get_contract_version(), 2);

        // Valid: large version number
        client.approve_upgrade(&bytes(&env, 2), &u32::MAX);
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
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);

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
        pc_client.approve_schema_version(&1);
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
        proof_client.register_proof(&proof_id_hash, &bytes(&env, 2), &issuer, &1, &2_000);
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
            proof_client.register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &2_000);
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
        pc_client.approve_schema_version(&1);

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
            proof_client.register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &2_000);
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
        pc_client.approve_schema_version(&1);

        let ir_client = IssuerRegistryContractClient::new(&env, &ir_id);
        ir_client.initialize(&admin);
        let issuer_id = bytes(&env, 9);
        ir_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8), &bytes(&env, 99));

        // Now proof registration should work because dependencies are initialized
        let proof_id_hash = bytes(&env, 1);
        proof_client.register_proof(&proof_id_hash, &bytes(&env, 2), &issuer, &1, &2_000);
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
            proof_client.register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &2_000);
        }));
        assert!(
            result.is_err(),
            "proof registration must fail without approved schema version"
        );

        // Now approve schema version but still no issuer registered
        pc_client.approve_schema_version(&1);

        // Proof registration should fail because issuer is not registered
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            proof_client.register_proof(&bytes(&env, 2), &bytes(&env, 3), &issuer, &1, &2_000);
        }));
        assert!(
            result.is_err(),
            "proof registration must fail with unregistered issuer"
        );

        // Now register the issuer and everything should work
        let issuer_id = bytes(&env, 9);
        ir_client.register_issuer(&issuer_id, &issuer, &bytes(&env, 8), &bytes(&env, 99));

        proof_client.register_proof(&bytes(&env, 3), &bytes(&env, 4), &issuer, &1, &2_000);
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
        pc_client.approve_schema_version(&1);
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
        proof_client.register_proof(&proof_id_hash, &bytes(&env, 2), &issuer, &1, &2_000);
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
        pc_client.pause();
        assert!(pc_client.is_paused());
        pc_client.unpause();
        assert!(!pc_client.is_paused());
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
        client.approve_upgrade(&hash, &2);
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

        client.register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &2_000);
        assert_eq!(client.get_registry_epoch(), 1);

        client.register_proof(&bytes(&env, 3), &bytes(&env, 4), &issuer, &1, &2_000);
        assert_eq!(client.get_registry_epoch(), 2);
    }

    #[test]
    fn revoke_proof_advances_the_epoch() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
        assert_eq!(client.get_registry_epoch(), 1);

        client.revoke_proof(&proof_id);
        assert_eq!(client.get_registry_epoch(), 2);
    }

    #[test]
    fn admin_revoke_proof_advances_the_epoch() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
        client.admin_revoke_proof(&proof_id);
        assert_eq!(client.get_registry_epoch(), 2);
    }

    #[test]
    fn a_rejected_registration_does_not_advance_the_epoch() {
        let (env, client, protocol_config, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol_config.pause();

        let result =
            client.try_register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &2_000);
        assert!(result.is_err());
        assert_eq!(client.get_registry_epoch(), 0);
    }

    #[test]
    fn a_double_revocation_does_not_advance_the_epoch_twice() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
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
            client.register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &2_000);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn registry_epoch_survives_an_upgrade() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &2_000);
        let epoch_before = client.get_registry_epoch();

        let hash = bytes(&env, 0x88);
        client.approve_upgrade(&hash, &2);
        client.upgrade_contract(&hash);

        assert_eq!(client.get_registry_epoch(), epoch_before);
    }

    // ── schema-specific payload size limits (issue #190) ─────────────────────

    #[test]
    fn register_proof_with_payload_accepts_a_zero_length_payload() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let proof_id = bytes(&env, 1);

        client.register_proof_with_payload(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
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
        client.register_proof_with_payload(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
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

        let result = client.try_register_proof_with_payload(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
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
        client.register_proof_with_payload(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
            &ok_payload,
        );
        assert!(client.is_valid_proof(&bytes(&env, 1)));

        let over_payload = soroban_sdk::Bytes::from_array(
            &env,
            &[0u8; (earnproof_shared::DEFAULT_SCHEMA_PAYLOAD_LIMIT + 1) as usize],
        );
        let result = client.try_register_proof_with_payload(
            &bytes(&env, 3),
            &bytes(&env, 4),
            &issuer,
            &1,
            &2_000,
            &over_payload,
        );
        assert_eq!(result, Err(Ok(ProofError::MalformedInput)));
    }

    #[test]
    fn register_proof_with_payload_rejects_a_used_deprecated_schema_the_same_as_registration_without_payload(
    ) {
        let (env, client, protocol_config, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        protocol_config.deprecate_schema_version(&1);

        let result = client.try_register_proof_with_payload(
            &bytes(&env, 1),
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
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

        let result = client.try_register_proof_with_payload(
            &proof_id,
            &bytes(&env, 2),
            &issuer,
            &1,
            &2_000,
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
        client.register_proof(&proof_id, &bytes(&env, 0xe6), &issuer, &1, &2_000);
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
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
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
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
        client.revoke_proof(&proof_id);
        assert_eq!(client.proof_validity(&proof_id), ProofValidity::Revoked);
    }

    #[test]
    fn proof_validity_reports_expired() {
        let (env, client, _pc, _ir, _ir_id) = setup();
        let proof_id = bytes(&env, 1);
        let issuer = Address::from_str(&env, ISSUER);
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
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
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
        // Suspend the issuer registered by setup (issuer_id == bytes 9).
        issuer_registry.suspend_issuer(&bytes(&env, 9));
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
        protocol_config.deprecate_schema_version(&1);
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
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
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
        issuer_registry.suspend_issuer(&bytes(&env, 9));
        protocol_config.deprecate_schema_version(&1);
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
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &5_000);
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
        client.register_proof(&proof_id, &bytes(&env, 2), &issuer, &1, &2_000);
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
            client.register_proof(&bytes(&env, 1), &bytes(&env, 2), &issuer, &1, &0);
        }));
        assert!(result.is_err());
        assert_eq!(env.events().all().events().len(), 0);
    }
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
        client.set_issuer_registry(&newer);
        assert_eq!(client.get_issuer_registry(), newer);
        assert_eq!(client.bound_issuer_registry_version().minor, 5);
    }

    #[test]
    fn governed_replacement_rejects_an_incompatible_dependency() {
        let (env, client, ..) = setup();
        let original = client.get_issuer_registry();
        let bad = env.register(IncompatibleDependency, ());

        let result = client.try_set_issuer_registry(&bad);
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
        protocol_config.pause();
        let bad = env.register(IncompatibleDependency, ());

        // Even while paused, the interface check still runs and rejects.
        let result = client.try_set_issuer_registry(&bad);
        assert_eq!(
            result,
            Err(Ok(
                earnproof_shared::ContractError::IncompatibleInterfaceVersion
            ))
        );
    }

    fn enabled_policy(env: &Env) -> (CriticalActionPolicy, Vec<Address>) {
        let signers = Vec::from_array(
            env,
            [
                Address::generate(env),
                Address::generate(env),
                Address::generate(env),
            ],
        );
        (
            CriticalActionPolicy {
                enabled: true,
                threshold: 2,
                signers: signers.clone(),
            },
            signers,
        )
    }

    #[test]
    fn dependency_replacement_requires_threshold_and_proposal_is_contract_bound() {
        let (env, client, _protocol_config, _issuer_registry, issuer_registry_id) = setup();
        let (policy, signers) = enabled_policy(&env);
        client.set_critical_action_policy(&policy);
        let newer = env.register(NewerCompatibleDependency, ());
        let proposal_id = client
            .propose_critical_action(&CriticalAction::IssuerRegistryReplacement(newer.clone()));

        assert_eq!(
            client.try_set_issuer_registry(&newer),
            Err(Ok(ContractError::ThresholdApprovalRequired))
        );
        client.approve_critical_action(&proposal_id, &signers.get(0).unwrap());
        assert_eq!(
            client.try_execute_critical_action(&proposal_id),
            Err(Ok(ContractError::InsufficientApprovals))
        );
        client.approve_critical_action(&proposal_id, &signers.get(1).unwrap());
        client.execute_critical_action(&proposal_id);
        assert_eq!(client.get_issuer_registry(), newer);
        assert_eq!(
            client.try_execute_critical_action(&proposal_id),
            Err(Ok(ContractError::ApprovalProposalNotFound))
        );

        let second_id = env.register(ProofRegistryContract, ());
        let second = ProofRegistryContractClient::new(&env, &second_id);
        second.initialize(
            &client.get_admin(),
            &issuer_registry_id,
            &client.get_protocol_config(),
        );
        assert_eq!(
            second.try_execute_critical_action(&proposal_id),
            Err(Ok(ContractError::ApprovalProposalNotFound))
        );
    }

    #[test]
    fn failed_dependency_execution_preserves_binding_and_approval() {
        let (env, client, ..) = setup();
        let (policy, signers) = enabled_policy(&env);
        client.set_critical_action_policy(&policy);
        let original = client.get_issuer_registry();
        let incompatible = env.register(IncompatibleDependency, ());
        let proposal_id = client
            .propose_critical_action(&CriticalAction::IssuerRegistryReplacement(incompatible));
        client.approve_critical_action(&proposal_id, &signers.get(0).unwrap());
        client.approve_critical_action(&proposal_id, &signers.get(1).unwrap());

        assert_eq!(
            client.try_execute_critical_action(&proposal_id),
            Err(Ok(ContractError::IncompatibleInterfaceVersion))
        );
        assert_eq!(client.get_issuer_registry(), original);
        assert!(client.get_critical_action_proposal(&proposal_id).is_some());
    }

    #[test]
    fn dependency_proposal_cancellation_and_expiry_are_terminal() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        let (policy, _) = enabled_policy(&env);
        client.set_critical_action_policy(&policy);
        let newer = env.register(NewerCompatibleDependency, ());
        let cancelled = client
            .propose_critical_action(&CriticalAction::IssuerRegistryReplacement(newer.clone()));
        client.cancel_critical_action(&cancelled);
        assert_eq!(
            client.try_execute_critical_action(&cancelled),
            Err(Ok(ContractError::ApprovalProposalNotFound))
        );

        let expiring =
            client.propose_critical_action(&CriticalAction::IssuerRegistryReplacement(newer));
        let proposal = client.get_critical_action_proposal(&expiring).unwrap();
        env.ledger()
            .set_sequence_number(proposal.created_at + CRITICAL_ACTION_APPROVAL_EXPIRY_LEDGERS);
        assert_eq!(
            client.try_execute_critical_action(&expiring),
            Err(Ok(ContractError::ApprovalProposalExpired))
        );
    }

    fn set_testnet_context(env: &Env) -> soroban_sdk::String {
        let passphrase = soroban_sdk::String::from_str(env, "Test SDF Network ; September 2015");
        env.ledger()
            .set_network_id(env.crypto().sha256(&passphrase.to_bytes()).to_array());
        passphrase
    }

    #[test]
    fn context_registration_derives_distinct_ids_and_preserves_legacy_records() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        let passphrase = set_testnet_context(&env);
        let issuer = Address::from_str(&env, ISSUER);
        let claim_id = bytes(&env, 0x51);
        let commitment = bytes(&env, 0x52);
        let raw_pseudonym = bytes(&env, 0x11);
        let pseudonym_commitment = earnproof_shared::compute_subject_pseudonym_commitment(
            &env,
            &issuer,
            &soroban_sdk::String::from_str(&env, "credential-verification"),
            &raw_pseudonym,
        )
        .unwrap();
        let native_id = client.register_proof_with_context(
            &claim_id,
            &commitment,
            &issuer,
            &1,
            &2_000,
            &earnproof_shared::ProofRegistrationContext {
                network_passphrase: passphrase.clone(),
                asset: earnproof_shared::ProofAssetIdentifier::Native,
                payload: None,
                subject_pseudonym_commitment: BytesN::from_array(&env, &[0; 32]),
            },
        );
        let issued_id = client.register_proof_with_context(
            &claim_id,
            &commitment,
            &issuer,
            &1,
            &2_000,
            &earnproof_shared::ProofRegistrationContext {
                network_passphrase: passphrase.clone(),
                asset: earnproof_shared::ProofAssetIdentifier::Issued(
                    soroban_sdk::String::from_str(&env, "USDC"),
                    issuer.clone(),
                ),
                payload: Some(soroban_sdk::Bytes::from_array(&env, &[0xA1, 0xB2])),
                subject_pseudonym_commitment: pseudonym_commitment.clone(),
            },
        );
        assert_ne!(native_id, issued_id);
        assert_eq!(client.get_proof(&native_id).proof_id_hash, native_id);
        assert_eq!(client.get_proof(&issued_id).proof_id_hash, issued_id);
        assert!(client.get_proof_context_commitments(&native_id).is_some());
        assert!(client.get_proof_context_commitments(&issued_id).is_some());
        assert_eq!(client.get_proof_pseudonym_commitment(&native_id), None);
        assert_eq!(
            client.get_proof_pseudonym_commitment(&issued_id),
            Some(pseudonym_commitment.clone())
        );
        let stored_pseudonym = env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .get::<_, BytesN<32>>(&DataKey::ProofSubjectPseudonym(issued_id.clone()))
                .unwrap()
        });
        assert_eq!(stored_pseudonym, pseudonym_commitment);
        assert_ne!(stored_pseudonym, raw_pseudonym);
        assert!(client.try_get_proof_payload(&issued_id).is_ok());

        let legacy_id = bytes(&env, 0x53);
        client.register_proof(&legacy_id, &commitment, &issuer, &1, &2_000);
        assert_eq!(client.get_proof_context_commitments(&legacy_id), None);
        assert_eq!(client.get_proof_pseudonym_commitment(&legacy_id), None);
        assert_eq!(
            client.try_register_proof_with_context(
                &claim_id,
                &commitment,
                &issuer,
                &1,
                &2_000,
                &earnproof_shared::ProofRegistrationContext {
                    network_passphrase: passphrase.clone(),
                    asset: earnproof_shared::ProofAssetIdentifier::Native,
                    payload: None,
                    subject_pseudonym_commitment: BytesN::from_array(&env, &[0; 32]),
                },
            ),
            Err(Ok(ProofError::ProofAlreadyRegistered))
        );
    }

    #[test]
    fn context_payload_registration_rejects_malformed_asset_without_writes() {
        let (env, client, _protocol_config, _issuer_registry, _issuer_registry_id) = setup();
        let passphrase = set_testnet_context(&env);
        let issuer = Address::from_str(&env, ISSUER);
        let result = client.try_register_proof_with_context(
            &bytes(&env, 0x61),
            &bytes(&env, 0x62),
            &issuer,
            &1,
            &2_000,
            &earnproof_shared::ProofRegistrationContext {
                network_passphrase: passphrase,
                asset: earnproof_shared::ProofAssetIdentifier::Issued(
                    soroban_sdk::String::from_str(&env, "USDC/USD"),
                    issuer,
                ),
                payload: Some(soroban_sdk::Bytes::new(&env)),
                subject_pseudonym_commitment: BytesN::from_array(&env, &[0; 32]),
            },
        );
        assert_eq!(result, Err(Ok(ProofError::InvalidProofContext)));
        assert_eq!(client.get_registry_epoch(), 0);
    }

    #[test]
    fn context_registration_requires_issuer_authorization() {
        let (env, client, ..) = setup();
        let passphrase = set_testnet_context(&env);
        let issuer = Address::from_str(&env, ISSUER);
        env.set_auths(&[]);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof_with_context(
                &bytes(&env, 0x71),
                &bytes(&env, 0x72),
                &issuer,
                &1,
                &2_000,
                &earnproof_shared::ProofRegistrationContext {
                    network_passphrase: passphrase,
                    asset: earnproof_shared::ProofAssetIdentifier::Native,
                    payload: None,
                    subject_pseudonym_commitment: BytesN::from_array(&env, &[0; 32]),
                },
            );
        }));
        assert!(result.is_err());
        assert_eq!(client.get_registry_epoch(), 0);
    }

    #[test]
    fn pseudonym_only_registration_uses_zero_as_absence() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        let zero_id = bytes(&env, 0x81);
        client.register_proof_with_pseudonym(
            &zero_id,
            &bytes(&env, 0x82),
            &issuer,
            &1,
            &2_000,
            &BytesN::from_array(&env, &[0; 32]),
        );
        assert_eq!(client.get_proof_pseudonym_commitment(&zero_id), None);

        let subject_id = bytes(&env, 0x83);
        let opaque_commitment = bytes(&env, 0x84);
        client.register_proof_with_pseudonym(
            &subject_id,
            &bytes(&env, 0x85),
            &issuer,
            &1,
            &2_000,
            &opaque_commitment,
        );
        assert_eq!(
            client.get_proof_pseudonym_commitment(&subject_id),
            Some(opaque_commitment)
        );
    }

    #[test]
    fn pseudonym_only_registration_requires_issuer_authorization() {
        let (env, client, ..) = setup();
        let issuer = Address::from_str(&env, ISSUER);
        env.set_auths(&[]);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_proof_with_pseudonym(
                &bytes(&env, 0x91),
                &bytes(&env, 0x92),
                &issuer,
                &1,
                &2_000,
                &bytes(&env, 0x93),
            );
        }));
        assert!(result.is_err());
        assert_eq!(client.get_registry_epoch(), 0);
        assert_eq!(
            client.try_get_proof_pseudonym_commitment(&bytes(&env, 0x91)),
            Err(Ok(ProofError::ProofNotFound))
        );
    }
}
