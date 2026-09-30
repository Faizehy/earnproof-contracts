/**
 * EarnProof Typed Contract Client
 *
 * AUTO-GENERATED — do not edit manually.
 * Regenerate with: npm run generate:bindings
 *
 * Replaces manual Stellar CLI shell-outs with type-safe invocations.
 * All method names, argument types, and return types are validated
 * at compile time against the contract spec.
 *
 * Usage (NestJS service):
 * ```typescript
 * import { EarnProofClient } from '../artifacts/bindings/client';
 *
 * const client = new EarnProofClient({
 *   protocolConfigId: process.env.PROTOCOL_CONFIG_ID,
 *   issuerRegistryId: process.env.ISSUER_REGISTRY_ID,
 *   proofRegistryId: process.env.PROOF_REGISTRY_ID,
 *   networkPassphrase: process.env.NETWORK_PASSPHRASE,
 *   rpcUrl: process.env.SOROBAN_RPC_URL,
 *   secretKey: process.env.SIGNER_SECRET_KEY,
 * });
 *
 * // Typed invocation — no manual argument building
 * const result = await client.registerProof({ params });
 * ```
 */

import * as StellarSDK from "@stellar/stellar-sdk";
import {
  Contract,
  Keypair,
  TransactionBuilder,
  Networks,
  BASE_FEE,
  Address,
  xdr,
  scValToNative,
  nativeToScVal,
  StrKey,
} from "@stellar/stellar-sdk";
import { Client as SorobanClient } from "@stellar/stellar-sdk/rpc";

import type {
  // Shared types
  IssuerStatus,
  ProofStatus,
  IssuerRecord,
  ProofRecord,
  ProofPolicySnapshot,
  SchemaPolicy,
  // Protocol Config params
  InitializeProtocolConfigParams,
  InitializeProtocolConfigResult,
  GetAdminProtocolConfigParams,
  GetAdminProtocolConfigResult,
  SetAdminParams,
  SetAdminResult,
  IsPausedParams,
  IsPausedResult,
  PauseParams,
  PauseResult,
  UnpauseParams,
  UnpauseResult,
  ApproveSchemaVersionParams,
  ApproveSchemaVersionResult,
  DeprecateSchemaVersionParams,
  DeprecateSchemaVersionResult,
  IsSchemaVersionApprovedParams,
  IsSchemaVersionApprovedResult,
  SetSchemaPolicyParams,
  SetSchemaPolicyResult,
  GetSchemaPolicyParams,
  GetSchemaPolicyResult,
  SetCommitmentAlgorithmParams,
  SetCommitmentAlgorithmResult,
  IsAlgorithmSupportedParams,
  IsAlgorithmSupportedResult,
  GetConfigVersionParams,
  GetConfigVersionResult,
  // Issuer Registry params
  InitializeIssuerRegistryParams,
  InitializeIssuerRegistryResult,
  GetAdminIssuerRegistryParams,
  GetAdminIssuerRegistryResult,
  RegisterIssuerParams,
  RegisterIssuerResult,
  UpdateIssuerParams,
  UpdateIssuerResult,
  SuspendIssuerParams,
  SuspendIssuerResult,
  ReactivateIssuerParams,
  ReactivateIssuerResult,
  RevokeIssuerParams,
  RevokeIssuerResult,
  RotateIssuerAddressParams,
  RotateIssuerAddressResult,
  GetIssuerParams,
  GetIssuerResult,
  IsActiveIssuerParams,
  IsActiveIssuerResult,
  IsActiveAddressParams,
  IsActiveAddressResult,
  GetIssuerByAddressParams,
  GetIssuerByAddressResult,
  // Proof Registry params
  InitializeProofRegistryParams,
  InitializeProofRegistryResult,
  RegisterProofParams,
  RegisterProofResult,
  RegisterProofWithPolicyParams,
  RegisterProofWithPolicyResult,
  RevokeProofParams,
  RevokeProofResult,
  AdminRevokeProofParams,
  AdminRevokeProofResult,
  GetProofParams,
  GetProofResult,
  GetProofPolicySnapshotParams,
  GetProofPolicySnapshotResult,
  IsValidProofParams,
  IsValidProofResult,
  IsRevokedParams,
  IsRevokedResult,
  GetAdminProofRegistryParams,
  GetAdminProofRegistryResult,
  GetIssuerRegistryParams,
  GetIssuerRegistryResult,
  GetProtocolConfigParams,
  GetProtocolConfigResult,
} from "./types";

/**
 * Configuration for EarnProofClient
 */
export interface EarnProofClientConfig {
  /** Protocol Config contract address — load from env, never hardcode */
  protocolConfigId: string;

  /** Issuer Registry contract address — load from env, never hardcode */
  issuerRegistryId: string;

  /** Proof Registry contract address — load from env, never hardcode */
  proofRegistryId: string;

  /** Network passphrase — load from env */
  networkPassphrase: string;

  /** Soroban RPC URL — load from env */
  rpcUrl: string;

  /** Source account secret key — load from env, never commit */
  secretKey: string;

  /** Optional timeout in ms (default: 30000) */
  timeoutMs?: number;
}

/**
 * Contract invocation error
 */
export class ContractInvocationError extends Error {
  constructor(
    public readonly method: string,
    public readonly contractId: string,
    message: string,
    public readonly originalError?: Error
  ) {
    super(`Contract invocation failed: ${method} on ${contractId}: ${message}`);
    this.name = "ContractInvocationError";
  }
}

/**
 * Typed client for EarnProof contracts
 *
 * Provides type-safe access to all contract methods with:
 * - Compile-time method name verification
 * - Parameter type checking
 * - Return type inference
 * - Automatic serialization/deserialization
 * - Transaction signing and submission
 */
export class EarnProofClient {
  private readonly protocolConfig: Contract;
  private readonly issuerRegistry: Contract;
  private readonly proofRegistry: Contract;
  private readonly server: SorobanClient;
  private readonly config: EarnProofClientConfig;
  private readonly keypair: Keypair;

  constructor(config: EarnProofClientConfig) {
    // Validate contract addresses
    this.validateContractId(config.protocolConfigId, "protocolConfigId");
    this.validateContractId(config.issuerRegistryId, "issuerRegistryId");
    this.validateContractId(config.proofRegistryId, "proofRegistryId");

    // Validate and parse secret key
    try {
      this.keypair = Keypair.fromSecret(config.secretKey);
    } catch (err) {
      throw new Error(
        "Invalid secret key: must be a valid Stellar secret key (starts with S)"
      );
    }

    this.config = config;
    this.protocolConfig = new Contract(config.protocolConfigId);
    this.issuerRegistry = new Contract(config.issuerRegistryId);
    this.proofRegistry = new Contract(config.proofRegistryId);
    this.server = new SorobanClient(config.rpcUrl);
  }

  /**
   * Validate contract address format
   */
  private validateContractId(contractId: string, fieldName: string): void {
    if (!/^C[A-Z2-7]{55}$/.test(contractId)) {
      throw new Error(
        `Invalid contract ID for ${fieldName}: must be a Stellar contract address (starts with C)`
      );
    }
  }

  /**
   * Convert bytes (BytesN<32>) to hex string
   */
  private bytesToHex(bytes: Buffer | Uint8Array): string {
    return "0x" + Buffer.from(bytes).toString("hex");
  }

  /**
   * Convert hex string to Buffer
   */
  private hexToBytes(hex: string): Buffer {
    const cleaned = hex.startsWith("0x") ? hex.slice(2) : hex;
    return Buffer.from(cleaned, "hex");
  }

  /**
   * Internal method to invoke a contract function
   * Handles simulation, signing, and transaction submission
   */
  private async invoke<T>(
    contract: Contract,
    contractId: string,
    method: string,
    args: xdr.ScVal[],
    parse: (val: xdr.ScVal) => T
  ): Promise<T> {
    try {
      const account = await this.server.getAccount(this.keypair.publicKey());

      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: this.config.networkPassphrase,
      })
        .addOperation(contract.call(method, ...args))
        .setTimeout(this.config.timeoutMs ?? 30000)
        .build();

      const sim = await this.server.simulateTransaction(tx);

      if (SorobanClient.isSimulationError(sim)) {
        throw new ContractInvocationError(
          method,
          contractId,
          `Simulation error: ${sim.error}`
        );
      }

      if (SorobanClient.isSimulationRestore(sim)) {
        // Handle state restoration if needed
        const restored = await this.server.prepareTransaction(tx, sim);
        restored.sign(this.keypair);
        const result = await this.server.sendTransaction(restored);
        return this.extractResult(result, method, contractId, parse);
      }

      const assembled = SorobanClient.assembleTransaction(tx, sim).build();
      assembled.sign(this.keypair);

      const result = await this.server.sendTransaction(assembled);
      return this.extractResult(result, method, contractId, parse);
    } catch (err) {
      if (err instanceof ContractInvocationError) {
        throw err;
      }
      throw new ContractInvocationError(
        method,
        contractId,
        err instanceof Error ? err.message : String(err),
        err instanceof Error ? err : undefined
      );
    }
  }

  /**
   * Extract result from transaction submission response
   */
  private extractResult<T>(
    result: SorobanClient.GetSuccessfulTransactionResponse,
    method: string,
    contractId: string,
    parse: (val: xdr.ScVal) => T
  ): T {
    if (result.status !== "success") {
      throw new ContractInvocationError(
        method,
        contractId,
        `Transaction failed with status: ${result.status}`
      );
    }

    if (!result.resultMetaXdr) {
      throw new ContractInvocationError(
        method,
        contractId,
        "No result metadata in transaction"
      );
    }

    const meta = xdr.TransactionMeta.fromXdr(
      result.resultMetaXdr,
      "base64"
    ).v3();
    if (!meta) {
      throw new ContractInvocationError(
        method,
        contractId,
        "Could not parse transaction metadata"
      );
    }

    const returnValue = meta.sorobanMeta()?.returnValue();
    if (!returnValue) {
      throw new ContractInvocationError(
        method,
        contractId,
        "No return value in contract result"
      );
    }

    return parse(returnValue);
  }

  // ────────────────────────────────────────────────────────────
  // PROTOCOL CONFIG CONTRACT
  // ────────────────────────────────────────────────────────────

  /**
   * Initialize protocol config contract
   * Requires admin authorization
   */
  async initializeProtocolConfig(
    params: InitializeProtocolConfigParams
  ): Promise<InitializeProtocolConfigResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "initialize",
      [nativeToScVal(params.admin, { type: "address" })],
      () => undefined
    );
  }

  /**
   * Get protocol admin address
   */
  async getAdminProtocolConfig(
    _params: GetAdminProtocolConfigParams = {}
  ): Promise<GetAdminProtocolConfigResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "get_admin",
      [],
      (val) => scValToNative(val) as string
    );
  }

  /**
   * Set new protocol admin
   * Requires current admin authorization
   */
  async setAdmin(params: SetAdminParams): Promise<SetAdminResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "set_admin",
      [nativeToScVal(params.new_admin, { type: "address" })],
      () => undefined
    );
  }

  /**
   * Check if protocol is paused
   */
  async isPaused(_params: IsPausedParams = {}): Promise<IsPausedResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "is_paused",
      [],
      (val) => scValToNative(val) as boolean
    );
  }

  /**
   * Pause protocol operations
   * Requires admin authorization
   */
  async pause(_params: PauseParams = {}): Promise<PauseResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "pause",
      [],
      () => undefined
    );
  }

  /**
   * Unpause protocol operations
   * Requires admin authorization
   */
  async unpause(_params: UnpauseParams = {}): Promise<UnpauseResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "unpause",
      [],
      () => undefined
    );
  }

  /**
   * Approve a credential schema version
   * Requires admin authorization
   */
  async approveSchemaVersion(
    params: ApproveSchemaVersionParams
  ): Promise<ApproveSchemaVersionResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "approve_schema_version",
      [nativeToScVal(params.version, { type: "u32" })],
      () => undefined
    );
  }

  /**
   * Deprecate a credential schema version
   * Requires admin authorization
   */
  async deprecateSchemaVersion(
    params: DeprecateSchemaVersionParams
  ): Promise<DeprecateSchemaVersionResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "deprecate_schema_version",
      [nativeToScVal(params.version, { type: "u32" })],
      () => undefined
    );
  }

  /**
   * Check if schema version is approved
   */
  async isSchemaVersionApproved(
    params: IsSchemaVersionApprovedParams
  ): Promise<IsSchemaVersionApprovedResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "is_schema_version_approved",
      [nativeToScVal(params.version, { type: "u32" })],
      (val) => scValToNative(val) as boolean
    );
  }

  async setSchemaPolicy(
    params: SetSchemaPolicyParams
  ): Promise<SetSchemaPolicyResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "set_schema_policy",
      [
        nativeToScVal(params.version, { type: "u32" }),
        nativeToScVal(params.proof_types, { type: "vec", elementType: "u32" }),
        nativeToScVal(params.max_validity_seconds, { type: "u64" }),
      ],
      () => undefined
    );
  }

  async getSchemaPolicy(
    params: GetSchemaPolicyParams
  ): Promise<GetSchemaPolicyResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "get_schema_policy",
      [nativeToScVal(params.version, { type: "u32" })],
      (val) => scValToNative(val) as SchemaPolicy
    );
  }

  async setCommitmentAlgorithm(
    params: SetCommitmentAlgorithmParams
  ): Promise<SetCommitmentAlgorithmResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "set_commitment_algorithm",
      [
        nativeToScVal(params.algorithm, { type: "u32" }),
        nativeToScVal(params.supported, { type: "bool" }),
      ],
      () => undefined
    );
  }

  async isAlgorithmSupported(
    params: IsAlgorithmSupportedParams
  ): Promise<IsAlgorithmSupportedResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "is_algorithm_supported",
      [nativeToScVal(params.algorithm, { type: "u32" })],
      (val) => scValToNative(val) as boolean
    );
  }

  /**
   * Get current protocol configuration version
   */
  async getConfigVersion(
    _params: GetConfigVersionParams = {}
  ): Promise<GetConfigVersionResult> {
    return this.invoke(
      this.protocolConfig,
      this.config.protocolConfigId,
      "get_config_version",
      [],
      (val) => scValToNative(val) as number
    );
  }

  // ────────────────────────────────────────────────────────────
  // ISSUER REGISTRY CONTRACT
  // ────────────────────────────────────────────────────────────

  /**
   * Initialize issuer registry contract
   * Requires admin authorization
   */
  async initializeIssuerRegistry(
    params: InitializeIssuerRegistryParams
  ): Promise<InitializeIssuerRegistryResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "initialize",
      [nativeToScVal(params.admin, { type: "address" })],
      () => undefined
    );
  }

  /**
   * Get issuer registry admin address
   */
  async getAdminIssuerRegistry(
    _params: GetAdminIssuerRegistryParams = {}
  ): Promise<GetAdminIssuerRegistryResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "get_admin",
      [],
      (val) => scValToNative(val) as string
    );
  }

  /**
   * Register a new issuer
   * Requires admin authorization
   */
  async registerIssuer(
    params: RegisterIssuerParams
  ): Promise<RegisterIssuerResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "register_issuer",
      [
        nativeToScVal(this.hexToBytes(params.issuer_id_hash), { type: "bytes" }),
        nativeToScVal(params.issuer_address, { type: "address" }),
        nativeToScVal(this.hexToBytes(params.metadata_hash), { type: "bytes" }),
      ],
      () => undefined
    );
  }

  /**
   * Update issuer metadata
   * Requires admin authorization
   */
  async updateIssuer(
    params: UpdateIssuerParams
  ): Promise<UpdateIssuerResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "update_issuer",
      [
        nativeToScVal(this.hexToBytes(params.issuer_id_hash), { type: "bytes" }),
        nativeToScVal(this.hexToBytes(params.metadata_hash), { type: "bytes" }),
      ],
      () => undefined
    );
  }

  /**
   * Suspend an issuer
   * Requires admin authorization
   */
  async suspendIssuer(
    params: SuspendIssuerParams
  ): Promise<SuspendIssuerResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "suspend_issuer",
      [nativeToScVal(this.hexToBytes(params.issuer_id_hash), { type: "bytes" })],
      () => undefined
    );
  }

  /**
   * Reactivate a suspended issuer
   * Requires admin authorization
   */
  async reactivateIssuer(
    params: ReactivateIssuerParams
  ): Promise<ReactivateIssuerResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "reactivate_issuer",
      [nativeToScVal(this.hexToBytes(params.issuer_id_hash), { type: "bytes" })],
      () => undefined
    );
  }

  /**
   * Revoke an issuer (terminal state)
   * Requires admin authorization
   */
  async revokeIssuer(
    params: RevokeIssuerParams
  ): Promise<RevokeIssuerResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "revoke_issuer",
      [nativeToScVal(this.hexToBytes(params.issuer_id_hash), { type: "bytes" })],
      () => undefined
    );
  }

  /**
   * Rotate issuer's Stellar address
   * Requires admin authorization
   */
  async rotateIssuerAddress(
    params: RotateIssuerAddressParams
  ): Promise<RotateIssuerAddressResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "rotate_issuer_address",
      [
        nativeToScVal(this.hexToBytes(params.issuer_id_hash), { type: "bytes" }),
        nativeToScVal(params.new_address, { type: "address" }),
      ],
      () => undefined
    );
  }

  /**
   * Get issuer record by issuer ID hash
   */
  async getIssuer(
    params: GetIssuerParams
  ): Promise<GetIssuerResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "get_issuer",
      [nativeToScVal(this.hexToBytes(params.issuer_id_hash), { type: "bytes" })],
      (val) => scValToNative(val) as IssuerRecord
    );
  }

  /**
   * Check if issuer is active by issuer ID hash
   */
  async isActiveIssuer(
    params: IsActiveIssuerParams
  ): Promise<IsActiveIssuerResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "is_active_issuer",
      [nativeToScVal(this.hexToBytes(params.issuer_id_hash), { type: "bytes" })],
      (val) => scValToNative(val) as boolean
    );
  }

  /**
   * Check if issuer is active by Stellar address
   */
  async isActiveAddress(
    params: IsActiveAddressParams
  ): Promise<IsActiveAddressResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "is_active_address",
      [nativeToScVal(params.issuer_address, { type: "address" })],
      (val) => scValToNative(val) as boolean
    );
  }

  /**
   * Get issuer record by Stellar address
   */
  async getIssuerByAddress(
    params: GetIssuerByAddressParams
  ): Promise<GetIssuerByAddressResult> {
    return this.invoke(
      this.issuerRegistry,
      this.config.issuerRegistryId,
      "get_issuer_by_address",
      [nativeToScVal(params.issuer_address, { type: "address" })],
      (val) => scValToNative(val) as IssuerRecord
    );
  }

  // ────────────────────────────────────────────────────────────
  // PROOF REGISTRY CONTRACT
  // ────────────────────────────────────────────────────────────

  /**
   * Initialize proof registry contract
   * Requires admin authorization
   */
  async initializeProofRegistry(
    params: InitializeProofRegistryParams
  ): Promise<InitializeProofRegistryResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "initialize",
      [
        nativeToScVal(params.admin, { type: "address" }),
        nativeToScVal(params.issuer_registry, { type: "address" }),
        nativeToScVal(params.protocol_config, { type: "address" }),
      ],
      () => undefined
    );
  }

  /**
   * Register a new proof commitment
   * Requires issuer authorization (issuer_address)
   */
  async registerProof(
    params: RegisterProofParams
  ): Promise<RegisterProofResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "register_proof",
      [
        nativeToScVal(this.hexToBytes(params.proof_id_hash), { type: "bytes" }),
        nativeToScVal(this.hexToBytes(params.commitment_hash), { type: "bytes" }),
        nativeToScVal(params.issuer_address, { type: "address" }),
        nativeToScVal(params.schema_version, { type: "u32" }),
        nativeToScVal(params.expires_at, { type: "u64" }),
      ],
      () => undefined
    );
  }

  async registerProofWithPolicy(
    params: RegisterProofWithPolicyParams
  ): Promise<RegisterProofWithPolicyResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "register_proof_with_policy",
      [
        nativeToScVal(this.hexToBytes(params.proof_id_hash), { type: "bytes" }),
        nativeToScVal(this.hexToBytes(params.commitment_hash), { type: "bytes" }),
        nativeToScVal(params.issuer_address, { type: "address" }),
        nativeToScVal(params.schema_version, { type: "u32" }),
        nativeToScVal(params.expires_at, { type: "u64" }),
        nativeToScVal(params.proof_type, { type: "u32" }),
        nativeToScVal(params.commitment_algorithm, { type: "u32" }),
      ],
      () => undefined
    );
  }

  /**
   * Revoke a proof (by issuer)
   * Requires issuer authorization
   */
  async revokeProof(
    params: RevokeProofParams
  ): Promise<RevokeProofResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "revoke_proof",
      [nativeToScVal(this.hexToBytes(params.proof_id_hash), { type: "bytes" })],
      () => undefined
    );
  }

  /**
   * Revoke a proof (by admin)
   * Requires admin authorization
   */
  async adminRevokeProof(
    params: AdminRevokeProofParams
  ): Promise<AdminRevokeProofResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "admin_revoke_proof",
      [nativeToScVal(this.hexToBytes(params.proof_id_hash), { type: "bytes" })],
      () => undefined
    );
  }

  /**
   * Get proof record by proof ID hash
   */
  async getProof(params: GetProofParams): Promise<GetProofResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "get_proof",
      [nativeToScVal(this.hexToBytes(params.proof_id_hash), { type: "bytes" })],
      (val) => scValToNative(val) as ProofRecord
    );
  }

  async getProofPolicySnapshot(
    params: GetProofPolicySnapshotParams
  ): Promise<GetProofPolicySnapshotResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "get_proof_policy_snapshot",
      [nativeToScVal(this.hexToBytes(params.proof_id_hash), { type: "bytes" })],
      (val) => scValToNative(val) as ProofPolicySnapshot
    );
  }

  /**
   * Check if proof is valid (not revoked and not expired)
   */
  async isValidProof(
    params: IsValidProofParams
  ): Promise<IsValidProofResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "is_valid_proof",
      [nativeToScVal(this.hexToBytes(params.proof_id_hash), { type: "bytes" })],
      (val) => scValToNative(val) as boolean
    );
  }

  /**
   * Check if proof is revoked
   */
  async isRevoked(params: IsRevokedParams): Promise<IsRevokedResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "is_revoked",
      [nativeToScVal(this.hexToBytes(params.proof_id_hash), { type: "bytes" })],
      (val) => scValToNative(val) as boolean
    );
  }

  /**
   * Get proof registry admin address
   */
  async getAdminProofRegistry(
    _params: GetAdminProofRegistryParams = {}
  ): Promise<GetAdminProofRegistryResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "get_admin",
      [],
      (val) => scValToNative(val) as string
    );
  }

  /**
   * Get issuer registry contract address
   */
  async getIssuerRegistry(
    _params: GetIssuerRegistryParams = {}
  ): Promise<GetIssuerRegistryResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "get_issuer_registry",
      [],
      (val) => scValToNative(val) as string
    );
  }

  /**
   * Get protocol config contract address
   */
  async getProtocolConfig(
    _params: GetProtocolConfigParams = {}
  ): Promise<GetProtocolConfigResult> {
    return this.invoke(
      this.proofRegistry,
      this.config.proofRegistryId,
      "get_protocol_config",
      [],
      (val) => scValToNative(val) as string
    );
  }
}

// Re-export types for consumer convenience
export type {
  // Shared types
  IssuerStatus,
  ProofStatus,
  IssuerRecord,
  ProofRecord,
  ProofPolicySnapshot,
  SchemaPolicy,
  // Protocol Config
  InitializeProtocolConfigParams,
  InitializeProtocolConfigResult,
  GetAdminProtocolConfigParams,
  GetAdminProtocolConfigResult,
  SetAdminParams,
  SetAdminResult,
  IsPausedParams,
  IsPausedResult,
  PauseParams,
  PauseResult,
  UnpauseParams,
  UnpauseResult,
  ApproveSchemaVersionParams,
  ApproveSchemaVersionResult,
  DeprecateSchemaVersionParams,
  DeprecateSchemaVersionResult,
  IsSchemaVersionApprovedParams,
  IsSchemaVersionApprovedResult,
  SetSchemaPolicyParams,
  SetSchemaPolicyResult,
  GetSchemaPolicyParams,
  GetSchemaPolicyResult,
  SetCommitmentAlgorithmParams,
  SetCommitmentAlgorithmResult,
  IsAlgorithmSupportedParams,
  IsAlgorithmSupportedResult,
  GetConfigVersionParams,
  GetConfigVersionResult,
  // Issuer Registry
  InitializeIssuerRegistryParams,
  InitializeIssuerRegistryResult,
  GetAdminIssuerRegistryParams,
  GetAdminIssuerRegistryResult,
  RegisterIssuerParams,
  RegisterIssuerResult,
  UpdateIssuerParams,
  UpdateIssuerResult,
  SuspendIssuerParams,
  SuspendIssuerResult,
  ReactivateIssuerParams,
  ReactivateIssuerResult,
  RevokeIssuerParams,
  RevokeIssuerResult,
  RotateIssuerAddressParams,
  RotateIssuerAddressResult,
  GetIssuerParams,
  GetIssuerResult,
  IsActiveIssuerParams,
  IsActiveIssuerResult,
  IsActiveAddressParams,
  IsActiveAddressResult,
  GetIssuerByAddressParams,
  GetIssuerByAddressResult,
  // Proof Registry
  InitializeProofRegistryParams,
  InitializeProofRegistryResult,
  RegisterProofParams,
  RegisterProofResult,
  RegisterProofWithPolicyParams,
  RegisterProofWithPolicyResult,
  RevokeProofParams,
  RevokeProofResult,
  AdminRevokeProofParams,
  AdminRevokeProofResult,
  GetProofParams,
  GetProofResult,
  GetProofPolicySnapshotParams,
  GetProofPolicySnapshotResult,
  IsValidProofParams,
  IsValidProofResult,
  IsRevokedParams,
  IsRevokedResult,
  GetAdminProofRegistryParams,
  GetAdminProofRegistryResult,
  GetIssuerRegistryParams,
  GetIssuerRegistryResult,
  GetProtocolConfigParams,
  GetProtocolConfigResult,
} from "./types";
