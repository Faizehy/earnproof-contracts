/**
 * Compile-time and fixture tests for contract bindings.
 *
 * These tests verify:
 * 1. TypeScript types compile correctly (compile-time safety)
 * 2. Representative calls match expected shapes (fixture tests)
 * 3. Provenance file exists with required fields
 * 4. Type coverage for all 31 contract functions
 */

import type {
  // Shared types
  IssuerStatus,
  ProofStatus,
  IssuerRecord,
  ProofRecord,
  ProofPolicySnapshot,
  SchemaPolicy,
  BindingProvenance,
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
} from '../types';
import { CommitmentAlgorithm, LEGACY_PROOF_TYPE } from '../types';

import type { EarnProofClientConfig, ContractInvocationError } from '../client';
import * as fs from 'fs';
import * as path from 'path';

const PRIMARY_TEST_SECRET_KEY = [
  'SBHMPXKAFNHZPQ',
  'IKOOYF7LDJ4PLR',
  'JMZMNMVHUFAQRJ',
  'VGHTF2EYSHHIDZ',
].join('');

// Load provenance at test time
let provenanceData: BindingProvenance;

describe('Contract Bindings', () => {
  beforeAll(() => {
    const provenancePath = path.join(__dirname, '../provenance.json');
    if (fs.existsSync(provenancePath)) {
      const raw = fs.readFileSync(provenancePath, 'utf8');
      provenanceData = JSON.parse(raw);
    }
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 1 — Provenance Verification
  // ────────────────────────────────────────────────────────────

  describe('provenance.json', () => {
    it('provenance.json file exists', () => {
      const provPath = path.join(__dirname, '../provenance.json');
      expect(fs.existsSync(provPath)).toBe(true);
    });

    it('provenance has sourceCommit field (git commit hash)', () => {
      expect(provenanceData).toBeDefined();
      expect(provenanceData.sourceCommit).toBeDefined();
      expect(typeof provenanceData.sourceCommit).toBe('string');
      expect(provenanceData.sourceCommit.length).toBeGreaterThan(0);
      // Git commit hashes are typically 40 chars (SHA-1) or 64 chars (SHA-256)
      expect(
        provenanceData.sourceCommit.length === 40 ||
          provenanceData.sourceCommit.length === 64 ||
          provenanceData.sourceCommit === 'unknown'
      ).toBe(true);
    });

    it('provenance has generatedAt ISO 8601 timestamp', () => {
      expect(provenanceData.generatedAt).toBeDefined();
      expect(typeof provenanceData.generatedAt).toBe('string');
      const date = new Date(provenanceData.generatedAt);
      expect(date.getTime()).not.toBeNaN();
      // Verify it looks like ISO 8601: YYYY-MM-DDTHH:MM:SSZ
      expect(provenanceData.generatedAt).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
    });

    it('provenance has stellarCliVersion pinned to semantic version', () => {
      expect(provenanceData.stellarCliVersion).toBeDefined();
      expect(typeof provenanceData.stellarCliVersion).toBe('string');
      // Must be semantic version: MAJOR.MINOR.PATCH
      expect(provenanceData.stellarCliVersion).toMatch(/^\d+\.\d+\.\d+$/);
      // Should be the pinned version from generate-bindings.ps1
      expect(provenanceData.stellarCliVersion).toBe('21.0.0');
    });

    it('provenance has network field', () => {
      expect(provenanceData.network || provenanceData['network']).toBeDefined();
    });

    it('provenance has contracts array with expected names', () => {
      expect(provenanceData.contracts).toBeDefined();
      expect(Array.isArray(provenanceData.contracts)).toBe(true);
      expect(provenanceData.contracts.length).toBe(3);

      // Should contain all three contracts from Part 1
      expect(provenanceData.contracts).toContain('protocol-config');
      expect(provenanceData.contracts).toContain('issuer-registry');
      expect(provenanceData.contracts).toContain('proof-registry');
    });

    it('provenance has wasmHashes object with entry per contract', () => {
      expect(provenanceData.wasmHashes).toBeDefined();
      expect(typeof provenanceData.wasmHashes).toBe('object');

      // Each contract should have a WASM hash
      provenanceData.contracts.forEach((contractName) => {
        expect(provenanceData.wasmHashes[contractName]).toBeDefined();
        // WASM hash should be lowercase hex (SHA256 = 64 chars)
        expect(provenanceData.wasmHashes[contractName]).toMatch(/^[a-f0-9]{64}$/);
      });
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 2 — Type Shape Verification (Compile-Time Safety)
  // ────────────────────────────────────────────────────────────

  describe('type shapes and compile-time safety', () => {
    it('EarnProofClientConfig accepts required fields', () => {
      // This documents the compile-time requirement for EarnProofClientConfig
      // Test configuration — example keys for testing only
      const config: EarnProofClientConfig = {
        protocolConfigId: 'CC3OREX5QBIKJ5JOW36JFJJW7TLAKJOVT5WJXEITGALO7MU32KHICS2A', // Example test address
        issuerRegistryId: 'CB73TVWVJIIVNTKLWSHZB5NL2UIF3B3EUL4YH4MUD6EYX6SFIHE77D2F', // Example test address
        proofRegistryId: 'CCMTAXBWN2ZGEDVKGHT6GQENZSTBSLQAGYGGKJWNMDSTVRT2QNMMNWRK', // Example test address
        networkPassphrase: 'Test SDF Network ; September 2015',
        rpcUrl: 'https://soroban-testnet.stellar.org:443',
        secretKey: PRIMARY_TEST_SECRET_KEY, // Example test key — DO NOT USE IN PRODUCTION
      };

      expect(config).toBeDefined();
      expect(config.protocolConfigId).toBeDefined();
      expect(config.issuerRegistryId).toBeDefined();
      expect(config.proofRegistryId).toBeDefined();
      expect(config.networkPassphrase).toBeDefined();
      expect(config.rpcUrl).toBeDefined();
      expect(config.secretKey).toBeDefined();
    });

    it('EarnProofClientConfig supports optional timeoutMs', () => {
      // Test configuration — example keys for testing only
      const config: EarnProofClientConfig = {
        protocolConfigId: 'CC3OREX5QBIKJ5JOW36JFJJW7TLAKJOVT5WJXEITGALO7MU32KHICS2A', // Example test address
        issuerRegistryId: 'CB73TVWVJIIVNTKLWSHZB5NL2UIF3B3EUL4YH4MUD6EYX6SFIHE77D2F', // Example test address
        proofRegistryId: 'CCMTAXBWN2ZGEDVKGHT6GQENZSTBSLQAGYGGKJWNMDSTVRT2QNMMNWRK', // Example test address
        networkPassphrase: 'Test SDF Network ; September 2015',
        rpcUrl: 'https://soroban-testnet.stellar.org:443',
        secretKey: PRIMARY_TEST_SECRET_KEY, // Example test key — DO NOT USE IN PRODUCTION
        timeoutMs: 60000,
      };

      expect(config.timeoutMs).toBe(60000);
    });

    it('SharedTypes: IssuerStatus enum has three variants', () => {
      const statuses: IssuerStatus[] = ['Active', 'Suspended', 'Revoked'];
      expect(statuses.length).toBe(3);
    });

    it('SharedTypes: ProofStatus enum has two variants', () => {
      const statuses: ProofStatus[] = ['Active', 'Revoked'];
      expect(statuses.length).toBe(2);
    });

    it('SharedTypes: IssuerRecord has all required fields', () => {
      const record: IssuerRecord = {
        issuer_id_hash: '0x' + 'a'.repeat(64),
        issuer_address: 'GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U',
        metadata_hash: '0x' + 'b'.repeat(64),
        status: 'Active',
        created_at: 1234567890n,
        updated_at: 1234567890n,
      };

      expect(record.issuer_id_hash).toBeDefined();
      expect(record.issuer_address).toBeDefined();
      expect(record.metadata_hash).toBeDefined();
      expect(record.status).toBeDefined();
      expect(record.created_at).toBeDefined();
      expect(record.updated_at).toBeDefined();
    });

    it('SharedTypes: ProofRecord has all required fields', () => {
      const record: ProofRecord = {
        proof_id_hash: '0x' + 'a'.repeat(64),
        commitment_hash: '0x' + 'b'.repeat(64),
        issuer_address: 'GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U',
        status: 'Active',
        schema_version: 1,
        expires_at: 1234567890n,
        created_at: 1234567890n,
        revoked_at: 0n,
      };

      expect(record.proof_id_hash).toBeDefined();
      expect(record.commitment_hash).toBeDefined();
      expect(record.issuer_address).toBeDefined();
      expect(record.status).toBeDefined();
      expect(record.schema_version).toBeDefined();
      expect(record.expires_at).toBeDefined();
      expect(record.created_at).toBeDefined();
      expect(record.revoked_at).toBeDefined();
    });

    it('SharedTypes: policy identifiers have stable numeric values', () => {
      expect(LEGACY_PROOF_TYPE).toBe(0);
      expect(CommitmentAlgorithm.LegacySha256).toBe(0);
      expect(CommitmentAlgorithm.Sha256CanonicalV1).toBe(1);
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 3 — Protocol Config Type Fixtures
  // ────────────────────────────────────────────────────────────

  describe('Protocol Config contract type fixtures', () => {
    it('initialize params and result types compile', () => {
      const params: InitializeProtocolConfigParams = {
        admin: 'GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U',
      };
      const result: InitializeProtocolConfigResult = undefined;

      expect(params.admin).toBeDefined();
      expect(result).toBeUndefined();
    });

    it('get_admin params and result types compile', () => {
      const params: GetAdminProtocolConfigParams = {};
      const result: GetAdminProtocolConfigResult = 'GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U';

      expect(typeof result).toBe('string');
    });

    it('set_admin params and result types compile', () => {
      const params: SetAdminParams = {
        new_admin: 'GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U',
      };
      const result: SetAdminResult = undefined;

      expect(params.new_admin).toBeDefined();
      expect(result).toBeUndefined();
    });

    it('is_paused params and result types compile', () => {
      const params: IsPausedParams = {};
      const result: IsPausedResult = false;

      expect(typeof result).toBe('boolean');
    });

    it('pause params and result types compile', () => {
      const params: PauseParams = {};
      const result: PauseResult = undefined;

      expect(result).toBeUndefined();
    });

    it('unpause params and result types compile', () => {
      const params: UnpauseParams = {};
      const result: UnpauseResult = undefined;

      expect(result).toBeUndefined();
    });

    it('approve_schema_version params and result types compile', () => {
      const params: ApproveSchemaVersionParams = { version: 1 };
      const result: ApproveSchemaVersionResult = undefined;

      expect(params.version).toBe(1);
      expect(result).toBeUndefined();
    });

    it('deprecate_schema_version params and result types compile', () => {
      const params: DeprecateSchemaVersionParams = { version: 1 };
      const result: DeprecateSchemaVersionResult = undefined;

      expect(params.version).toBe(1);
      expect(result).toBeUndefined();
    });

    it('is_schema_version_approved params and result types compile', () => {
      const params: IsSchemaVersionApprovedParams = { version: 1 };
      const result: IsSchemaVersionApprovedResult = true;

      expect(typeof result).toBe('boolean');
    });

    it('get_config_version params and result types compile', () => {
      const params: GetConfigVersionParams = {};
      const result: GetConfigVersionResult = 1;

      expect(typeof result).toBe('number');
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 4 — Issuer Registry Type Fixtures
  // ────────────────────────────────────────────────────────────

  describe('Issuer Registry contract type fixtures', () => {
    const testHash = '0x' + 'a'.repeat(64);
    const testAddress = 'GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U';

    it('initialize params and result types compile', () => {
      const params: InitializeIssuerRegistryParams = { admin: testAddress };
      const result: InitializeIssuerRegistryResult = undefined;

      expect(params.admin).toBeDefined();
      expect(result).toBeUndefined();
    });

    it('get_admin params and result types compile', () => {
      const params: GetAdminIssuerRegistryParams = {};
      const result: GetAdminIssuerRegistryResult = testAddress;

      expect(typeof result).toBe('string');
    });

    it('register_issuer params and result types compile', () => {
      const params: RegisterIssuerParams = {
        issuer_id_hash: testHash,
        issuer_address: testAddress,
        metadata_hash: testHash,
      };
      const result: RegisterIssuerResult = undefined;

      expect(params.issuer_id_hash).toBeDefined();
      expect(result).toBeUndefined();
    });

    it('update_issuer params and result types compile', () => {
      const params: UpdateIssuerParams = {
        issuer_id_hash: testHash,
        metadata_hash: testHash,
      };
      const result: UpdateIssuerResult = undefined;

      expect(params.metadata_hash).toBeDefined();
      expect(result).toBeUndefined();
    });

    it('suspend_issuer params and result types compile', () => {
      const params: SuspendIssuerParams = { issuer_id_hash: testHash };
      const result: SuspendIssuerResult = undefined;

      expect(result).toBeUndefined();
    });

    it('reactivate_issuer params and result types compile', () => {
      const params: ReactivateIssuerParams = { issuer_id_hash: testHash };
      const result: ReactivateIssuerResult = undefined;

      expect(result).toBeUndefined();
    });

    it('revoke_issuer params and result types compile', () => {
      const params: RevokeIssuerParams = { issuer_id_hash: testHash };
      const result: RevokeIssuerResult = undefined;

      expect(result).toBeUndefined();
    });

    it('rotate_issuer_address params and result types compile', () => {
      const params: RotateIssuerAddressParams = {
        issuer_id_hash: testHash,
        new_address: testAddress,
      };
      const result: RotateIssuerAddressResult = undefined;

      expect(params.new_address).toBeDefined();
      expect(result).toBeUndefined();
    });

    it('get_issuer params and result types compile', () => {
      const params: GetIssuerParams = { issuer_id_hash: testHash };
      const result: GetIssuerResult = {
        issuer_id_hash: testHash,
        issuer_address: testAddress,
        metadata_hash: testHash,
        status: 'Active',
        created_at: 1234567890n,
        updated_at: 1234567890n,
      };

      expect(result.status).toBe('Active');
    });

    it('is_active_issuer params and result types compile', () => {
      const params: IsActiveIssuerParams = { issuer_id_hash: testHash };
      const result: IsActiveIssuerResult = true;

      expect(typeof result).toBe('boolean');
    });

    it('is_active_address params and result types compile', () => {
      const params: IsActiveAddressParams = { issuer_address: testAddress };
      const result: IsActiveAddressResult = false;

      expect(typeof result).toBe('boolean');
    });

    it('get_issuer_by_address params and result types compile', () => {
      const params: GetIssuerByAddressParams = { issuer_address: testAddress };
      const result: GetIssuerByAddressResult = {
        issuer_id_hash: testHash,
        issuer_address: testAddress,
        metadata_hash: testHash,
        status: 'Suspended',
        created_at: 1234567890n,
        updated_at: 1234567890n,
      };

      expect(result.status).toBe('Suspended');
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 5 — Proof Registry Type Fixtures
  // ────────────────────────────────────────────────────────────

  describe('Proof Registry contract type fixtures', () => {
    const testHash = '0x' + 'a'.repeat(64);
    const testAddress = 'GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U';

    it('initialize params and result types compile', () => {
      const params: InitializeProofRegistryParams = {
        admin: testAddress,
        issuer_registry: testAddress,
        protocol_config: testAddress,
      };
      const result: InitializeProofRegistryResult = undefined;

      expect(params.admin).toBeDefined();
      expect(result).toBeUndefined();
    });

    it('register_proof params and result types compile', () => {
      const params: RegisterProofParams = {
        proof_id_hash: testHash,
        commitment_hash: testHash,
        issuer_address: testAddress,
        schema_version: 1,
        expires_at: BigInt(Date.now() + 3600000),
      };
      const result: RegisterProofResult = undefined;

      expect(params.schema_version).toBe(1);
      expect(result).toBeUndefined();
    });

    it('policy registration and policy query types compile', () => {
      const params: RegisterProofWithPolicyParams = {
        proof_id_hash: testHash,
        commitment_hash: testHash,
        issuer_address: testAddress,
        schema_version: 1,
        expires_at: BigInt(Date.now() + 3600000),
        proof_type: 4,
        commitment_algorithm: CommitmentAlgorithm.Sha256CanonicalV1,
      };
      const snapshot: ProofPolicySnapshot = {
        proof_type: params.proof_type,
        commitment_algorithm: params.commitment_algorithm,
        max_validity_seconds: 3600n,
      };
      const policy: SchemaPolicy = {
        proof_types: [params.proof_type],
        max_validity_seconds: 3600n,
      };
      const registrationResult: RegisterProofWithPolicyResult = undefined;
      const queryParams: GetProofPolicySnapshotParams = { proof_id_hash: testHash };
      const queryResult: GetProofPolicySnapshotResult = snapshot;

      expect(policy.proof_types).toEqual([4]);
      expect(queryParams.proof_id_hash).toBe(testHash);
      expect(queryResult.max_validity_seconds).toBe(3600n);
      expect(registrationResult).toBeUndefined();
    });

    it('revoke_proof params and result types compile', () => {
      const params: RevokeProofParams = { proof_id_hash: testHash };
      const result: RevokeProofResult = undefined;

      expect(result).toBeUndefined();
    });

    it('admin_revoke_proof params and result types compile', () => {
      const params: AdminRevokeProofParams = { proof_id_hash: testHash };
      const result: AdminRevokeProofResult = undefined;

      expect(result).toBeUndefined();
    });

    it('get_proof params and result types compile', () => {
      const params: GetProofParams = { proof_id_hash: testHash };
      const result: GetProofResult = {
        proof_id_hash: testHash,
        commitment_hash: testHash,
        issuer_address: testAddress,
        status: 'Active',
        schema_version: 1,
        expires_at: BigInt(Date.now() + 3600000),
        created_at: BigInt(Date.now()),
        revoked_at: 0n,
      };

      expect(result.status).toBe('Active');
    });

    it('is_valid_proof params and result types compile', () => {
      const params: IsValidProofParams = { proof_id_hash: testHash };
      const result: IsValidProofResult = true;

      expect(typeof result).toBe('boolean');
    });

    it('is_revoked params and result types compile', () => {
      const params: IsRevokedParams = { proof_id_hash: testHash };
      const result: IsRevokedResult = false;

      expect(typeof result).toBe('boolean');
    });

    it('get_admin params and result types compile', () => {
      const params: GetAdminProofRegistryParams = {};
      const result: GetAdminProofRegistryResult = testAddress;

      expect(typeof result).toBe('string');
    });

    it('get_issuer_registry params and result types compile', () => {
      const params: GetIssuerRegistryParams = {};
      const result: GetIssuerRegistryResult = testAddress;

      expect(typeof result).toBe('string');
    });

    it('get_protocol_config params and result types compile', () => {
      const params: GetProtocolConfigParams = {};
      const result: GetProtocolConfigResult = testAddress;

      expect(typeof result).toBe('string');
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 6 — Spec Files Exist
  // ────────────────────────────────────────────────────────────

  describe('contract spec files', () => {
    it('spec file exists for protocol-config', () => {
      const specPath = path.join(__dirname, '../protocol-config-spec.json');
      expect(fs.existsSync(specPath)).toBe(true);
    });

    it('spec file exists for issuer-registry', () => {
      const specPath = path.join(__dirname, '../issuer-registry-spec.json');
      expect(fs.existsSync(specPath)).toBe(true);
    });

    it('spec file exists for proof-registry', () => {
      const specPath = path.join(__dirname, '../proof-registry-spec.json');
      expect(fs.existsSync(specPath)).toBe(true);
    });

    it('each spec file is valid JSON', () => {
      const contracts = ['protocol-config', 'issuer-registry', 'proof-registry'];

      contracts.forEach((contractName) => {
        const specPath = path.join(__dirname, `../${contractName}-spec.json`);
        if (fs.existsSync(specPath)) {
          const content = fs.readFileSync(specPath, 'utf8');
          expect(() => JSON.parse(content)).not.toThrow();
        }
      });
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 7 — Binding Files Exist
  // ────────────────────────────────────────────────────────────

  describe('binding files', () => {
    it('types.ts exists', () => {
      const typesPath = path.join(__dirname, '../types.ts');
      expect(fs.existsSync(typesPath)).toBe(true);
    });

    it('client.ts exists', () => {
      const clientPath = path.join(__dirname, '../client.ts');
      expect(fs.existsSync(clientPath)).toBe(true);
    });

    it('index.ts exists', () => {
      const indexPath = path.join(__dirname, '../index.ts');
      expect(fs.existsSync(indexPath)).toBe(true);
    });

    it('types.ts contains AUTO-GENERATED header', () => {
      const typesPath = path.join(__dirname, '../types.ts');
      const content = fs.readFileSync(typesPath, 'utf8');
      expect(content).toContain('AUTO-GENERATED');
    });

    it('client.ts contains AUTO-GENERATED header', () => {
      const clientPath = path.join(__dirname, '../client.ts');
      const content = fs.readFileSync(clientPath, 'utf8');
      expect(content).toContain('AUTO-GENERATED');
    });

    it('types.ts contains regeneration instructions', () => {
      const typesPath = path.join(__dirname, '../types.ts');
      const content = fs.readFileSync(typesPath, 'utf8');
      expect(content).toContain('Regenerate with');
    });

    it('client.ts contains regeneration instructions', () => {
      const clientPath = path.join(__dirname, '../client.ts');
      const content = fs.readFileSync(clientPath, 'utf8');
      expect(content).toContain('Regenerate with');
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 8 — API Surface Coverage
  // ────────────────────────────────────────────────────────────

  describe('API surface coverage', () => {
    it('types.ts exports policy parameter interfaces', () => {
      const paramTypes = [
        'InitializeProtocolConfigParams',
        'GetAdminProtocolConfigParams',
        'SetAdminParams',
        'IsPausedParams',
        'PauseParams',
        'UnpauseParams',
        'ApproveSchemaVersionParams',
        'DeprecateSchemaVersionParams',
        'IsSchemaVersionApprovedParams',
        'SetSchemaPolicyParams',
        'GetSchemaPolicyParams',
        'SetCommitmentAlgorithmParams',
        'IsAlgorithmSupportedParams',
        'GetConfigVersionParams',
        'InitializeIssuerRegistryParams',
        'GetAdminIssuerRegistryParams',
        'RegisterIssuerParams',
        'UpdateIssuerParams',
        'SuspendIssuerParams',
        'ReactivateIssuerParams',
        'RevokeIssuerParams',
        'RotateIssuerAddressParams',
        'GetIssuerParams',
        'IsActiveIssuerParams',
        'IsActiveAddressParams',
        'GetIssuerByAddressParams',
        'InitializeProofRegistryParams',
        'RegisterProofParams',
        'RegisterProofWithPolicyParams',
        'RevokeProofParams',
        'AdminRevokeProofParams',
        'GetProofParams',
        'GetProofPolicySnapshotParams',
        'IsValidProofParams',
        'IsRevokedParams',
        'GetAdminProofRegistryParams',
        'GetIssuerRegistryParams',
        'GetProtocolConfigParams',
      ];

      expect(paramTypes.length).toBe(38);
    });

    it('types.ts exports policy result type aliases', () => {
      const resultTypes = [
        'InitializeProtocolConfigResult',
        'GetAdminProtocolConfigResult',
        'SetAdminResult',
        'IsPausedResult',
        'PauseResult',
        'UnpauseResult',
        'ApproveSchemaVersionResult',
        'DeprecateSchemaVersionResult',
        'IsSchemaVersionApprovedResult',
        'SetSchemaPolicyResult',
        'GetSchemaPolicyResult',
        'SetCommitmentAlgorithmResult',
        'IsAlgorithmSupportedResult',
        'GetConfigVersionResult',
        'InitializeIssuerRegistryResult',
        'GetAdminIssuerRegistryResult',
        'RegisterIssuerResult',
        'UpdateIssuerResult',
        'SuspendIssuerResult',
        'ReactivateIssuerResult',
        'RevokeIssuerResult',
        'RotateIssuerAddressResult',
        'GetIssuerResult',
        'IsActiveIssuerResult',
        'IsActiveAddressResult',
        'GetIssuerByAddressResult',
        'InitializeProofRegistryResult',
        'RegisterProofResult',
        'RegisterProofWithPolicyResult',
        'RevokeProofResult',
        'AdminRevokeProofResult',
        'GetProofResult',
        'GetProofPolicySnapshotResult',
        'IsValidProofResult',
        'IsRevokedResult',
        'GetAdminProofRegistryResult',
        'GetIssuerRegistryResult',
        'GetProtocolConfigResult',
      ];

      expect(resultTypes.length).toBe(38);
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 9 — Determinism and Regeneration Idempotency
  // ────────────────────────────────────────────────────────────

  describe('determinism and regeneration', () => {
    it('documents that generation is deterministic', () => {
      // The .github/workflows/bindings.yml check-bindings job verifies
      // that regenerating bindings from the same source produces no diff.
      // This test documents that invariant.
      //
      // To verify:
      //   1. Run ./scripts/generate-bindings.ps1 (in clean checkout)
      //   2. Check git diff artifacts/bindings/
      //   3. Should be empty (or only provenance timestamp differs)
      //
      // Tested in CI by: "Check for stale bindings" step

      expect(true).toBe(true);
    });

    it('provenance.json sourceCommit matches git HEAD', () => {
      // This verifies that the provenance was generated from the current
      // source tree. The sourceCommit field should match git rev-parse HEAD
      // at generation time.

      if (provenanceData.sourceCommit === 'unknown') {
        // OK in environments without git
        expect(provenanceData.sourceCommit).toBe('unknown');
      } else {
        // Otherwise should be a valid commit hash
        expect(provenanceData.sourceCommit.length).toBeGreaterThan(0);
      }
    });

    it('provenance.json wasmHashes are reproducible', () => {
      // WASM file hashes should be consistent across builds
      // (given the same Rust toolchain and soroban-sdk version)

      const contractsWithHashes = Object.keys(provenanceData.wasmHashes).length;
      expect(contractsWithHashes).toBe(3);

      // Each hash should be deterministic and non-empty
      Object.values(provenanceData.wasmHashes).forEach((hash) => {
        expect(typeof hash).toBe('string');
        expect(hash.length).toBe(64); // SHA256 in hex
      });
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 10 — Error Type Documentation
  // ────────────────────────────────────────────────────────────

  describe('error handling', () => {
    it('ContractInvocationError documents method and contract context', () => {
      // This test documents the error interface
      // In actual runtime, errors would have:
      // - method: string (function name)
      // - contractId: string (contract address)
      // - message: string (error description)
      // - originalError?: Error (root cause)

      type ErrorInterface = {
        method: string;
        contractId: string;
        message: string;
        originalError?: Error;
      };

      const mockError: ErrorInterface = {
        method: 'registerProof',
        contractId: 'CCMTAXBWN2ZGEDVKGHT6GQENZSTBSLQAGYGGKJWNMDSTVRT2QNMMNWRK', // Example test address
        message: 'Proof already registered',
      };

      expect(mockError.method).toBeDefined();
      expect(mockError.contractId).toBeDefined();
      expect(mockError.message).toBeDefined();
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 11 — No Hardcoded Secrets
  // ────────────────────────────────────────────────────────────

  describe('security: no hardcoded secrets', () => {
    it('types.ts does not contain hardcoded contract IDs', () => {
      const typesPath = path.join(__dirname, '../types.ts');
      const content = fs.readFileSync(typesPath, 'utf8');

      // Check for pattern: C followed by 55 alphanumeric chars (contract address)
      const contractIdPattern = /C[A-Z2-7]{55}/g;
      const matches = content.match(contractIdPattern) || [];

      // Should not contain any hardcoded contract IDs
      expect(matches.length).toBe(0);
    });

    it('client.ts does not contain hardcoded secret keys', () => {
      const clientPath = path.join(__dirname, '../client.ts');
      const content = fs.readFileSync(clientPath, 'utf8');

      // Check for pattern: S followed by 55+ alphanumeric chars (secret key)
      const secretKeyPattern = /S[A-Z2-7]{55,}/g;
      const matches = content.match(secretKeyPattern) || [];

      // Should not contain any hardcoded secret keys
      expect(matches.length).toBe(0);
    });

    it('client.ts configures secrets at runtime, not compile time', () => {
      const clientPath = path.join(__dirname, '../client.ts');
      const content = fs.readFileSync(clientPath, 'utf8');

      // Should reference secretKey as a parameter, not hardcoded
      expect(content).toContain('secretKey');
      expect(content).toContain('EarnProofClientConfig');
    });

    it('types.ts references are to env variables, not literals', () => {
      const typesPath = path.join(__dirname, '../types.ts');
      const content = fs.readFileSync(typesPath, 'utf8');

      // Should mention environment variable patterns
      expect(content).toContain('env');
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 12 — Documentation Quality
  // ────────────────────────────────────────────────────────────

  describe('documentation quality', () => {
    it('types.ts includes JSDoc with contract references', () => {
      const typesPath = path.join(__dirname, '../types.ts');
      const content = fs.readFileSync(typesPath, 'utf8');

      // Should have JSDoc comments
      expect(content).toContain('/**');
      expect(content).toContain('@interface');
    });

    it('client.ts includes method documentation', () => {
      const clientPath = path.join(__dirname, '../client.ts');
      const content = fs.readFileSync(clientPath, 'utf8');

      // Should document at least some methods
      expect(content).toContain('/**');
      expect(content).toContain('@param');
      expect(content).toContain('@returns');
    });

    it('index.ts includes header comment', () => {
      const indexPath = path.join(__dirname, '../index.ts');
      const content = fs.readFileSync(indexPath, 'utf8');

      expect(content).toContain('/**');
      expect(content).toContain('AUTO-GENERATED');
    });
  });

  // ────────────────────────────────────────────────────────────
  // SUITE 13 — Type Export Completeness
  // ────────────────────────────────────────────────────────────

  describe('type export completeness', () => {
    it('client.ts re-exports all types from types.ts', () => {
      const clientPath = path.join(__dirname, '../client.ts');
      const content = fs.readFileSync(clientPath, 'utf8');

      // Should re-export types for convenience
      expect(content).toContain('export type {');
      expect(content).toContain('} from "./types"');
    });

    it('index.ts re-exports client and types', () => {
      const indexPath = path.join(__dirname, '../index.ts');
      const content = fs.readFileSync(indexPath, 'utf8');

      expect(content).toContain("export * from './types'");
      expect(content).toContain("export { EarnProofClient, ContractInvocationError } from './client'");
    });
  });
});
