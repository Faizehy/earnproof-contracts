# Deployment Manifest Schema Versioning

This document describes what `manifestVersion` means, how it is incremented, and how consumers and
operators should handle breaking schema changes.

## What `manifestVersion` Represents

`manifestVersion` is a required integer field in every EarnProof deployment manifest
(`scripts/*.json`). It identifies the structural version of the manifest format — not the version
of the deployed contracts, and not the on-chain schema version tracked by `schemaVersions`.

The initial value is **1**. It is incremented once per breaking schema change and never reset.

```json
{
  "manifestVersion": 1,
  "network": "stellar-testnet",
  ...
}
```

The field must always be the first key in the JSON object so that readers can detect the version
without parsing the full document.

## Breaking vs Additive Changes

### Breaking Changes (require a version bump)

A breaking change is any modification to `schemas/deployment-manifest.schema.json` that causes a
previously valid manifest to become invalid, or that changes the meaning of an existing field in
a way that consumers cannot ignore. Breaking changes require bumping `manifestVersion`.

Examples of breaking changes:

- Removing a field from the `required` array and then expecting it absent (or vice versa — adding
  a new field to `required`).
- Narrowing a pattern constraint, e.g. changing `^[a-fA-F0-9]{64}$` to require only lowercase
  `^[a-f0-9]{64}$`.
- Changing a field's type, e.g. `schemaVersions` from `array` to `string`.
- Renaming a field (removing one required field and adding a differently named required field).
- Tightening an enum to exclude a value that existing manifests may contain.

### Additive Changes (no version bump required)

An additive change is one that all existing valid manifests continue to satisfy after the change
is applied. No version bump is required.

Examples of additive changes:

- Adding a new optional property to `properties` without adding it to `required`.
- Widening a pattern constraint, e.g. relaxing a length check.
- Adding a new allowed value to an `enum`.
- Expanding the `additionalProperties` on an optional sub-object.
- Adding a new entry to the secret-field `patternProperties` block (new forbidden key name).

## Migration Procedure

When a breaking schema change is necessary, follow these steps in order:

1. **Bump `manifestVersion`** in the schema's description and in the version history table below.
2. **Update `schemas/deployment-manifest.schema.json`** with the breaking change.
3. **Update all manifest files** (`scripts/deployment-manifest.testnet.json`,
   `scripts/deployment-manifest.example.json`, and any environment-specific manifests) to conform
   to the new schema and reflect the new `manifestVersion` value.
4. **Update CI fixtures** (`tests/fixtures/valid-manifest.json`) to match the updated testnet
   manifest. Optionally update `tests/fixtures/invalid-manifest.json` if the new schema adds
   additional rejection cases worth documenting.
5. **Add a migration note** to the version history table in this document describing what changed,
   why, and any action required by consumers.
6. **Verify end-to-end** by running:

   ```bash
   npx ajv-cli@5 validate \
     -s schemas/deployment-manifest.schema.json \
     -d "scripts/*.json" \
     --spec=draft7 \
     --all-errors
   ```

   The command must exit 0 before merging.

## How CI Enforces the Schema

The `schema-validation` job in `.github/workflows/ci.yml` runs `npx ajv-cli@5 validate` against
every `scripts/*.json` file on every push. If any manifest file fails the schema, CI fails and
the PR cannot be merged.

The validator is invoked without a Node.js setup step because `npx` is available on
`ubuntu-latest`. The exact command used in CI is:

```yaml
npx ajv-cli@5 validate \
  -s schemas/deployment-manifest.schema.json \
  -d "scripts/*.json" \
  --spec=draft7 \
  --all-errors
```

## Version History

| Version | Date | Description |
|---------|------|-------------|
| 1 | 2026-09-28 | Initial schema. Required fields: `manifestVersion`, `network`, `contracts`, `wasm`, `admin`, `initialIssuer`, `schemaVersions`, `deployedAt`. Enforces G-address, C-address, SHA-256 patterns, network enum, and top-level secret-field prohibition. |

## Schema Location

The canonical schema is at:

```
schemas/deployment-manifest.schema.json
```

Its `$id` is:

```
https://github.com/earnproof/earnproof-contracts/schemas/deployment-manifest.schema.json
```

## Related Documents

- [Deployment](deployment.md) — how to run a testnet deployment
- [Contract Upgrades](contract-upgrades.md) — upgrade procedure and manifest updates
- [Storage Model](storage-model.md) — on-chain storage and the privacy boundary
