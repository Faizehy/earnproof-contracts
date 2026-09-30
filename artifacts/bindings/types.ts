/**
 * EarnProof Contract TypeScript Bindings
 *
 * AUTO-GENERATED — do not edit manually.
 * Source commit: injected at generation time
 * WASM hashes: injected at generation time
 *
 * Regenerate with: npm run generate:bindings
 *
 * Provenance:
 * - Source: veridatum-labs/earnproof-contracts
 * - Soroban SDK version: 27.0.0
 * - Generated at: (timestamp)
 */

// ── Shared Contract Types ─────────────────────────────────────

/**
 * IssuerStatus enumeration.
 * Represents the current operational status of an issuer.
 */
export enum IssuerStatus {
  Active = "Active",
  Suspended = "Suspended",
  Revoked = "Revoked",
}

/**
 * ProofStatus enumeration.
 * Represents the current operational status of a proof commitment.
 */
export enum ProofStatus {
  Active = "Active",
  Revoked = "Revoked",
}

/** Stable commitment algorithm identifiers stored with each proof. */
export enum CommitmentAlgorithm {
  LegacySha256 = 0,
  Sha256CanonicalV1 = 1,
}

/** Reserved proof type for the original registration entrypoints. */
export const LEGACY_PROOF_TYPE = 0;
export const MAX_SCHEMA_PROOF_TYPES = 16;
export const MAX_SCHEMA_VALIDITY_SECONDS = 3_153_600_000n;

/**
 * IssuerRecord structure.
 * Complete record for an issuer including identity, status, and timestamps.
 */
export interface IssuerRecord {
  issuer_id_hash: string; // BytesN<32> as hex string
  issuer_address: string; // Stellar address
  metadata_hash: string; // BytesN<32> as hex string
  status: IssuerStatus;
  created_at: bigint; // u64 timestamp
  updated_at: bigint; // u64 timestamp
}

/**
 * ProofRecord structure.
 * Complete record for a proof commitment including validation state.
 */
export interface ProofRecord {
  proof_id_hash: string; // BytesN<32> as hex string
  commitment_hash: string; // BytesN<32> as hex string
  disclosure_policy_hash: string; // BytesN<32> as hex string; zero means legacy/unset
  issuer_address: string; // Stellar address
  status: ProofStatus;
  schema_version: number; // u32
  expires_at: bigint; // u64 timestamp
  created_at: bigint; // u64 timestamp
  revoked_at: bigint; // u64 timestamp (0 if not revoked)
  proof_type?: string; // Option<BytesN<32>> as hex string
}

/** Immutable proof type and commitment interpretation selected at registration. */
export interface ProofPolicySnapshot {
  proof_type: number;
  commitment_algorithm: CommitmentAlgorithm;
  max_validity_seconds: bigint;
}

/** Bounded policy configured for one schema version. */
export interface SchemaPolicy {
  proof_types: number[];
  max_validity_seconds: bigint;
}

// ── Protocol Config Contract Types ───────────────────────────

/**
 * Parameters for protocol_config::initialize
 */
export interface InitializeProtocolConfigParams {
  admin: string; // Address
}

/**
 * Return type: Address
 */
export type InitializeProtocolConfigResult = void;

/**
 * Parameters for protocol_config::get_admin
 */
export interface GetAdminProtocolConfigParams {
  // No parameters
}

/**
 * Return type: Address
 */
export type GetAdminProtocolConfigResult = string;

/**
 * Parameters for protocol_config::set_admin
 */
export interface SetAdminParams {
  new_admin: string; // Address
}

/**
 * Return type: void
 */
export type SetAdminResult = void;

/**
 * Parameters for protocol_config::is_paused
 */
export interface IsPausedParams {
  // No parameters
}

/**
 * Return type: bool
 */
export type IsPausedResult = boolean;

/**
 * Parameters for protocol_config::pause
 */
export interface PauseParams {
  // No parameters
}

/**
 * Return type: void
 */
export type PauseResult = void;

/**
 * Parameters for protocol_config::unpause
 */
export interface UnpauseParams {
  // No parameters
}

/**
 * Return type: void
 */
export type UnpauseResult = void;

/**
 * Parameters for protocol_config::approve_schema_version
 */
export interface ApproveSchemaVersionParams {
  version: number; // u32
}

/**
 * Return type: void
 */
export type ApproveSchemaVersionResult = void;

/**
 * Parameters for protocol_config::deprecate_schema_version
 */
export interface DeprecateSchemaVersionParams {
  version: number; // u32
}

/**
 * Return type: void
 */
export type DeprecateSchemaVersionResult = void;

/**
 * Parameters for protocol_config::is_schema_version_approved
 */
export interface IsSchemaVersionApprovedParams {
  version: number; // u32
}

/**
 * Return type: bool
 */
export type IsSchemaVersionApprovedResult = boolean;

/** Parameters for protocol_config::set_schema_policy */
export interface SetSchemaPolicyParams {
  version: number;
  proof_types: number[];
  max_validity_seconds: bigint;
}
export type SetSchemaPolicyResult = void;

/** Parameters for protocol_config::get_schema_policy */
export interface GetSchemaPolicyParams {
  version: number;
}
export type GetSchemaPolicyResult = SchemaPolicy;

/** Parameters for protocol_config::set_commitment_algorithm */
export interface SetCommitmentAlgorithmParams {
  algorithm: CommitmentAlgorithm;
  supported: boolean;
}
export type SetCommitmentAlgorithmResult = void;

/** Parameters for protocol_config::is_algorithm_supported */
export interface IsAlgorithmSupportedParams {
  algorithm: CommitmentAlgorithm;
}
export type IsAlgorithmSupportedResult = boolean;

/**
 * Parameters for protocol_config::keepalive_proof_type
 */
export interface KeepaliveProofTypeParams {
  proof_type: string; // BytesN<32> as hex string
}

/**
 * Return type: bool
 */
export type KeepaliveProofTypeResult = boolean;

/**
 * Parameters for protocol_config::approve_proof_type
 */
export interface ApproveProofTypeParams {
  proof_type: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type ApproveProofTypeResult = void;

/**
 * Parameters for protocol_config::deprecate_proof_type
 */
export interface DeprecateProofTypeParams {
  proof_type: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type DeprecateProofTypeResult = void;

/**
 * Parameters for protocol_config::is_proof_type_approved
 */
export interface IsProofTypeApprovedParams {
  proof_type: string; // BytesN<32> as hex string
}

/**
 * Return type: bool
 */
export type IsProofTypeApprovedResult = boolean;

/**
 * Parameters for protocol_config::get_config_version
 */
export interface GetConfigVersionParams {
  // No parameters
}

/**
 * Return type: u32
 */
export type GetConfigVersionResult = number;

// ── Issuer Registry Contract Types ──────────────────────────

/**
 * Parameters for issuer_registry::initialize
 */
export interface InitializeIssuerRegistryParams {
  admin: string; // Address
}

/**
 * Return type: void
 */
export type InitializeIssuerRegistryResult = void;

/**
 * Parameters for issuer_registry::get_admin
 */
export interface GetAdminIssuerRegistryParams {
  // No parameters
}

/**
 * Return type: Address
 */
export type GetAdminIssuerRegistryResult = string;

/**
 * Parameters for issuer_registry::register_issuer
 */
export interface RegisterIssuerParams {
  issuer_id_hash: string; // BytesN<32> as hex string
  issuer_address: string; // Address
  metadata_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type RegisterIssuerResult = void;

/**
 * Parameters for issuer_registry::update_issuer
 */
export interface UpdateIssuerParams {
  issuer_id_hash: string; // BytesN<32> as hex string
  metadata_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type UpdateIssuerResult = void;

/**
 * Parameters for issuer_registry::suspend_issuer
 */
export interface SuspendIssuerParams {
  issuer_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type SuspendIssuerResult = void;

/**
 * Parameters for issuer_registry::reactivate_issuer
 */
export interface ReactivateIssuerParams {
  issuer_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type ReactivateIssuerResult = void;

/**
 * Parameters for issuer_registry::revoke_issuer
 */
export interface RevokeIssuerParams {
  issuer_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type RevokeIssuerResult = void;

/**
 * Parameters for issuer_registry::rotate_issuer_address
 */
export interface RotateIssuerAddressParams {
  issuer_id_hash: string; // BytesN<32> as hex string
  new_address: string; // Address
}

/**
 * Return type: void
 */
export type RotateIssuerAddressResult = void;

/**
 * Parameters for issuer_registry::get_issuer
 */
export interface GetIssuerParams {
  issuer_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: IssuerRecord
 */
export type GetIssuerResult = IssuerRecord;

/**
 * Parameters for issuer_registry::is_active_issuer
 */
export interface IsActiveIssuerParams {
  issuer_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: bool
 */
export type IsActiveIssuerResult = boolean;

/**
 * Parameters for issuer_registry::is_active_address
 */
export interface IsActiveAddressParams {
  issuer_address: string; // Address
}

/**
 * Return type: bool
 */
export type IsActiveAddressResult = boolean;

/**
 * Parameters for issuer_registry::get_issuer_by_address
 */
export interface GetIssuerByAddressParams {
  issuer_address: string; // Address
}

/**
 * Return type: IssuerRecord
 */
export type GetIssuerByAddressResult = IssuerRecord;

// ── Proof Registry Contract Types ───────────────────────────

/**
 * Parameters for proof_registry::initialize
 */
export interface InitializeProofRegistryParams {
  admin: string; // Address
  issuer_registry: string; // Address
  protocol_config: string; // Address
}

/**
 * Return type: void
 */
export type InitializeProofRegistryResult = void;

/**
 * Parameters for proof_registry::register_proof
 */
export interface RegisterProofParams {
  proof_id_hash: string; // BytesN<32> as hex string
  commitment_hash: string; // BytesN<32> as hex string
  issuer_address: string; // Address
  schema_version: number; // u32
  expires_at: bigint; // u64 timestamp
  proof_type: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type RegisterProofResult = void;

/** Parameters for proof_registry::register_proof_with_policy */
export interface RegisterProofWithPolicyParams extends Omit<RegisterProofParams, "proof_type"> {
  proof_type: number;
  commitment_algorithm: CommitmentAlgorithm;
}
export type RegisterProofWithPolicyResult = void;

/**
 * Parameters for proof_registry::revoke_proof
 */
export interface RevokeProofParams {
  proof_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type RevokeProofResult = void;

/**
 * Parameters for proof_registry::admin_revoke_proof
 */
export interface AdminRevokeProofParams {
  proof_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: void
 */
export type AdminRevokeProofResult = void;

/**
 * Parameters for proof_registry::get_proof
 */
export interface GetProofParams {
  proof_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: ProofRecord
 */
export type GetProofResult = ProofRecord;

/** Parameters for proof_registry::get_proof_policy_snapshot */
export interface GetProofPolicySnapshotParams {
  proof_id_hash: string;
}
export type GetProofPolicySnapshotResult = ProofPolicySnapshot;

/**
 * Parameters for proof_registry::is_valid_proof
 */
export interface IsValidProofParams {
  proof_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: bool
 */
export type IsValidProofResult = boolean;

/**
 * Parameters for proof_registry::is_revoked
 */
export interface IsRevokedParams {
  proof_id_hash: string; // BytesN<32> as hex string
}

/**
 * Return type: bool
 */
export type IsRevokedResult = boolean;

/**
 * Parameters for proof_registry::get_admin
 */
export interface GetAdminProofRegistryParams {
  // No parameters
}

/**
 * Return type: Address
 */
export type GetAdminProofRegistryResult = string;

/**
 * Parameters for proof_registry::get_issuer_registry
 */
export interface GetIssuerRegistryParams {
  // No parameters
}

/**
 * Return type: Address
 */
export type GetIssuerRegistryResult = string;

/**
 * Parameters for proof_registry::get_protocol_config
 */
export interface GetProtocolConfigParams {
  // No parameters
}

/**
 * Return type: Address
 */
export type GetProtocolConfigResult = string;

// ── Provenance ────────────────────────────────────────────────

/**
 * Build provenance for traceability.
 * Ensures bindings were generated from a known source state.
 */
export interface BindingProvenance {
  /** Git commit hash of the contract source */
  sourceCommit: string;

  /** Timestamps of generated artifacts */
  generatedAt: string;

  /** Version of Stellar CLI used for generation */
  stellarCliVersion: string;

  /** Contracts included in this binding */
  contractNames: string[];

  /** WASM hashes for each contract */
  wasmHashes: {
    [contractName: string]: string;
  };
}

// ── Stellar SDK Type Aliases ──────────────────────────────────

/**
 * Stellar account address (56 chars starting with G)
 */
export type StellarAddress = string & { readonly __brand: "StellarAddress" };

/**
 * 32-byte hash as hex string (64 chars)
 */
export type Hash32 = string & { readonly __brand: "Hash32" };

/**
 * Helper to create branded types with runtime validation
 */
export function asStellarAddress(value: string): StellarAddress {
  if (!/^G[A-Z2-7]{55}$/.test(value)) {
    throw new Error(`Invalid Stellar address: ${value}`);
  }
  return value as StellarAddress;
}

/**
 * Helper to create branded Hash32 type with validation
 */
export function asHash32(value: string): Hash32 {
  if (!/^[a-f0-9]{64}$/.test(value)) {
    throw new Error(`Invalid 32-byte hash: ${value}`);
  }
  return value as Hash32;
}
