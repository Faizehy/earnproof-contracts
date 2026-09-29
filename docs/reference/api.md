# Generated Contract API

<!-- BEGIN GENERATED: do not edit. -->

## issuer-registry::activate_successor

- Parameters: `env: Env`
- Result: `Result<(), IssuerError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::advance_migration

- Parameters: `env: Env, expected_cursor: u32, processed_items: u32,`
- Result: `Result<MigrationStatus, ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::approve_upgrade

- Parameters: `env: Env, wasm_hash: BytesN<32>, new_version: u32,`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::begin_migration

- Parameters: `env: Env, target_contract_version: u32, total_items: u32,`
- Result: `Result<MigrationStatus, ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_address_ttl_status

- Parameters: `env: Env, issuer_address: Address`
- Result: `TtlStatus`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_admin

- Parameters: `env: Env`
- Result: `Result<Address, ContractError>`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_config_digest

- Parameters: `env: Env`
- Result: `Result<BytesN<32>, ContractError>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_config_digest_version

- Parameters: ``
- Result: `u32`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_contract_version

- Parameters: `env: Env`
- Result: `u32`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_genesis

- Parameters: `env: Env`
- Result: `Result<GenesisRecord, ContractError>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_instance_ttl_status

- Parameters: `env: Env`
- Result: `TtlStatus`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_issuer

- Parameters: `env: Env, issuer_id_hash: BytesN<32>`
- Result: `Result<IssuerRecord, IssuerError>`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_issuer_by_address

- Parameters: `env: Env, issuer_address: Address,`
- Result: `Result<IssuerRecord, IssuerError>`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_issuer_ttl_status

- Parameters: `env: Env, issuer_id_hash: BytesN<32>`
- Result: `TtlStatus`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_latest_upgrade_receipt

- Parameters: `env: Env`
- Result: `Option<UpgradeReceipt>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_migration_status

- Parameters: `env: Env`
- Result: `Option<MigrationStatus>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_provenance_commitment

- Parameters: `env: Env, issuer_id_hash: BytesN<32>,`
- Result: `Result<BytesN<32>, IssuerError>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::get_successor

- Parameters: `env: Env`
- Result: `Option<Address>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::initialize

- Parameters: `env: Env, admin: Address`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::is_active_address

- Parameters: `env: Env, issuer_address: Address`
- Result: `bool`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::is_active_issuer

- Parameters: `env: Env, issuer_id_hash: BytesN<32>`
- Result: `bool`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::is_decommissioned

- Parameters: `env: Env`
- Result: `bool`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::is_upgrade_allowed

- Parameters: `env: Env, wasm_hash: BytesN<32>`
- Result: `bool`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::keepalive_address_issuer

- Parameters: `env: Env, issuer_address: Address`
- Result: `bool`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::keepalive_instance

- Parameters: `env: Env`
- Result: `bool`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::keepalive_issuer

- Parameters: `env: Env, issuer_id_hash: BytesN<32>`
- Result: `bool`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::nominate_successor

- Parameters: `env: Env, successor: Address`
- Result: `Result<(), IssuerError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::reactivate_issuer

- Parameters: `env: Env, issuer_id_hash: BytesN<32>`
- Result: `Result<(), IssuerError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::refresh_instance_ttl

- Parameters: `env: Env`
- Result: `Result<TtlStatus, ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::register_issuer

- Parameters: `env: Env, issuer_id_hash: BytesN<32>, issuer_address: Address, metadata_hash: BytesN<32>, provenance_commitment: BytesN<32>,`
- Result: `Result<(), IssuerError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::revoke_issuer

- Parameters: `env: Env, issuer_id_hash: BytesN<32>`
- Result: `Result<(), IssuerError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::revoke_upgrade

- Parameters: `env: Env, wasm_hash: BytesN<32>`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::revoke_upgrade_approval

- Parameters: `env: Env`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::rotate_issuer_address

- Parameters: `env: Env, issuer_id_hash: BytesN<32>, new_address: Address,`
- Result: `Result<(), IssuerError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::suspend_issuer

- Parameters: `env: Env, issuer_id_hash: BytesN<32>`
- Result: `Result<(), IssuerError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::update_issuer

- Parameters: `env: Env, issuer_id_hash: BytesN<32>, metadata_hash: BytesN<32>,`
- Result: `Result<(), IssuerError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## issuer-registry::upgrade_contract

- Parameters: `env: Env, wasm_hash: BytesN<32>, new_version: u32,`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/issuer-registry/src/lib.rs`

## proof-registry::activate_successor

- Parameters: `env: Env`
- Result: `Result<(), ProofError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::admin_revoke_proof

- Parameters: `env: Env, proof_id_hash: BytesN<32>`
- Result: `Result<(), ProofError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::advance_migration

- Parameters: `env: Env, expected_cursor: u32, processed_items: u32,`
- Result: `Result<MigrationStatus, ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::approve_upgrade

- Parameters: `env: Env, wasm_hash: BytesN<32>, new_version: u32`
- Result: `()`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::begin_migration

- Parameters: `env: Env, target_contract_version: u32, total_items: u32,`
- Result: `Result<MigrationStatus, ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_admin

- Parameters: `env: Env`
- Result: `Result<Address, ContractError>`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_contract_version

- Parameters: `env: Env`
- Result: `u32`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_genesis

- Parameters: `env: Env`
- Result: `Result<GenesisRecord, ContractError>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_issuer_registry

- Parameters: `env: Env`
- Result: `Result<Address, ContractError>`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_migration_status

- Parameters: `env: Env`
- Result: `Option<MigrationStatus>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_proof

- Parameters: `env: Env, proof_id_hash: BytesN<32>`
- Result: `Result<ProofRecord, ProofError>`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_proof_payload

- Parameters: `env: Env, proof_id_hash: BytesN<32>,`
- Result: `Result<ProofPayloadRecord, ProofError>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_protocol_config

- Parameters: `env: Env`
- Result: `Result<Address, ContractError>`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_registry_epoch

- Parameters: `env: Env`
- Result: `u32`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::get_successor

- Parameters: `env: Env`
- Result: `Option<Address>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::initialize

- Parameters: `env: Env, admin: Address, issuer_registry: Address, protocol_config: Address,`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::is_decommissioned

- Parameters: `env: Env`
- Result: `bool`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::is_revoked

- Parameters: `env: Env, proof_id_hash: BytesN<32>`
- Result: `bool`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::is_upgrade_allowed

- Parameters: `env: Env, wasm_hash: BytesN<32>`
- Result: `bool`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::is_valid_proof

- Parameters: `env: Env, proof_id_hash: BytesN<32>`
- Result: `bool`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::keepalive_instance

- Parameters: `env: Env`
- Result: `bool`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::keepalive_proof

- Parameters: `env: Env, proof_id_hash: BytesN<32>`
- Result: `bool`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::nominate_successor

- Parameters: `env: Env, successor: Address`
- Result: `Result<(), ProofError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::register_proof

- Parameters: `env: Env, proof_id_hash: BytesN<32>, commitment_hash: BytesN<32>, issuer_address: Address, schema_version: u32, expires_at: u64,`
- Result: `Result<(), ProofError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::register_proof_with_payload

- Parameters: `env: Env, proof_id_hash: BytesN<32>, commitment_hash: BytesN<32>, issuer_address: Address, schema_version: u32, expires_at: u64, payload: Bytes,`
- Result: `Result<(), ProofError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::revoke_proof

- Parameters: `env: Env, proof_id_hash: BytesN<32>`
- Result: `Result<(), ProofError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::revoke_upgrade

- Parameters: `env: Env, wasm_hash: BytesN<32>`
- Result: `()`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## proof-registry::upgrade_contract

- Parameters: `env: Env, wasm_hash: BytesN<32>`
- Result: `()`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/proof-registry/src/lib.rs`

## protocol-config::activate_successor

- Parameters: `env: Env`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::advance_migration

- Parameters: `env: Env, expected_cursor: u32, processed_items: u32,`
- Result: `Result<MigrationStatus, ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::approve_schema_version

- Parameters: `env: Env, version: u32`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::approve_upgrade

- Parameters: `env: Env, wasm_hash: BytesN<32>, new_version: u32`
- Result: `()`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::begin_migration

- Parameters: `env: Env, target_contract_version: u32, total_items: u32,`
- Result: `Result<MigrationStatus, ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::deprecate_schema_version

- Parameters: `env: Env, version: u32`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_admin

- Parameters: `env: Env`
- Result: `Result<Address, ContractError>`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_config_history

- Parameters: `env: Env, cursor: u32, limit: u32`
- Result: `Vec<ConfigChangeSummary>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_config_history_cursor

- Parameters: `env: Env`
- Result: `u32`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_config_version

- Parameters: `env: Env`
- Result: `u32`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_contract_version

- Parameters: `env: Env`
- Result: `u32`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_genesis

- Parameters: `env: Env`
- Result: `Result<GenesisRecord, ContractError>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_migration_status

- Parameters: `env: Env`
- Result: `Option<MigrationStatus>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_schema_payload_limit

- Parameters: `env: Env, version: u32`
- Result: `u32`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::get_successor

- Parameters: `env: Env`
- Result: `Option<Address>`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::initialize

- Parameters: `env: Env, admin: Address`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::is_decommissioned

- Parameters: `env: Env`
- Result: `bool`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::is_paused

- Parameters: `env: Env`
- Result: `bool`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::is_schema_version_approved

- Parameters: `env: Env, version: u32`
- Result: `bool`
- Authorization: none
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::is_scope_paused

- Parameters: `env: Env, scope: PauseScope`
- Result: `bool`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::is_upgrade_allowed

- Parameters: `env: Env, wasm_hash: BytesN<32>`
- Result: `bool`
- Authorization: current admin
- Storage effect: read-only
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::keepalive_instance

- Parameters: `env: Env`
- Result: `bool`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::keepalive_schema_version

- Parameters: `env: Env, version: u32`
- Result: `bool`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::nominate_successor

- Parameters: `env: Env, successor: Address`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::pause

- Parameters: `env: Env`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::revoke_upgrade

- Parameters: `env: Env, wasm_hash: BytesN<32>`
- Result: `()`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::set_admin

- Parameters: `env: Env, new_admin: Address`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::set_schema_payload_limit

- Parameters: `env: Env, version: u32, max_size: u32,`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::set_scoped_pause

- Parameters: `env: Env, scope: PauseScope, paused: bool,`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::unpause

- Parameters: `env: Env`
- Result: `Result<(), ContractError>`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

## protocol-config::upgrade_contract

- Parameters: `env: Env, wasm_hash: BytesN<32>`
- Result: `()`
- Authorization: current admin
- Storage effect: documented in lifecycle specification
- Event effect: none unless documented in lifecycle specification
- Failure atomicity: Soroban invocation rollback
- Source: `contracts/protocol-config/src/lib.rs`

<!-- END GENERATED -->
