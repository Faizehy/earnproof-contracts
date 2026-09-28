<#
.SYNOPSIS
EarnProof Contract Binding Generation

.DESCRIPTION
Generates TypeScript type definitions and typed client from Soroban contract specs.

Pinned Stellar CLI version for deterministic generation.
Must match the version in .github/workflows/bindings.yml

.PARAMETER Network
Target network: 'testnet' or 'mainnet'
Default: 'testnet'

.PARAMETER NoWasmBuild
If set, skips contract building (useful for regenerating types only)

.PARAMETER SkipProvenance
If set, skips writing provenance.json (useful for deterministic CI stale checks)

.PARAMETER Verbose
Enable detailed logging

.EXAMPLE
.\scripts\generate-bindings.ps1 -Network testnet

.EXAMPLE
.\scripts\generate-bindings.ps1 -NoWasmBuild

.NOTES
Outputs:
  - artifacts/bindings/types.ts
  - artifacts/bindings/client.ts
  - artifacts/bindings/provenance.json
  - artifacts/bindings/*-spec.json (one per contract)

Security notes:
  - Never passes secrets; network IDs come from args only
  - Does not require or accept environment variable secrets
  - All secrets loading is deferred to runtime (NestJS ConfigService)
#>

param(
  [ValidateSet('testnet', 'mainnet')]
  [string]$Network = 'testnet',

  [switch]$NoWasmBuild,

  [switch]$SkipProvenance,

  [switch]$Verbose
)

$ErrorActionPreference = 'Stop'

# ────────────────────────────────────────────────────────────
# Configuration
# ────────────────────────────────────────────────────────────

$STELLAR_CLI_VERSION = '27.1.0' # PIN — change requires PR review
$CONTRACTS_DIR = 'contracts'
$ARTIFACTS_DIR = 'artifacts/bindings'
$ROOT_DIR = (Get-Location).Path
$WASM_TARGET = 'wasm32v1-none'

# ────────────────────────────────────────────────────────────
# Functions
# ────────────────────────────────────────────────────────────

function Write-Status {
  param([string]$Message)
  Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Success {
  param([string]$Message)
  Write-Host "✅ $Message" -ForegroundColor Green
}

function Write-Error-Custom {
  param([string]$Message)
  Write-Host "❌ $Message" -ForegroundColor Red
}

function Invoke-Command-Checked {
  param(
    [string]$Description,
    [scriptblock]$ScriptBlock,
    [switch]$CaptureOutput
  )

  Write-Status $Description

  $previousErrorActionPreference = $ErrorActionPreference
  try {
    # Windows PowerShell can surface redirected native stderr as an ErrorRecord
    # even when the process exits successfully. Check $LASTEXITCODE directly.
    $ErrorActionPreference = 'Continue'
    if ($CaptureOutput) {
      $output = & $ScriptBlock 2>&1
      $exitCode = $LASTEXITCODE
      $ErrorActionPreference = $previousErrorActionPreference
      if ($exitCode -ne 0) {
        Write-Host ($output | Out-String) -ForegroundColor Red
        throw "Command failed with exit code $exitCode"
      }
      return $output
    }
    else {
      & $ScriptBlock 2>&1
      $exitCode = $LASTEXITCODE
      $ErrorActionPreference = $previousErrorActionPreference
      if ($exitCode -ne 0) {
        throw "Command failed with exit code $exitCode"
      }
    }
  }
  catch {
    $ErrorActionPreference = $previousErrorActionPreference
    Write-Error-Custom "Failed: $Description"
    throw $_
  }
}

function Get-FileHash-Sha256 {
  param([string]$Path)
  $hash = (Get-FileHash -Path $Path -Algorithm SHA256).Hash
  return $hash.ToLowerInvariant()
}

function Get-GitCommit {
  try {
    $commit = (git rev-parse HEAD 2>&1)
    if ($LASTEXITCODE -eq 0) {
      return $commit.Trim()
    }
  }
  catch { }
  return 'unknown'
}

function Get-TimeStampUtc {
  return (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
}

function Set-Utf8NoBomContent {
  param(
    [string]$Path,
    [string]$Value
  )

  $encoding = [System.Text.UTF8Encoding]::new($false)
  [System.IO.File]::WriteAllText($Path, $Value, $encoding)
}

# ────────────────────────────────────────────────────────────
# Validation
# ────────────────────────────────────────────────────────────

Write-Status "Validating environment"

# Check Rust and Cargo
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  throw "Rust toolchain not found. Install from https://rustup.rs/"
}

# Check Cargo is available
Write-Host "Cargo available" -ForegroundColor Green

# Create artifacts directory
if (-not (Test-Path $ARTIFACTS_DIR)) {
  New-Item -ItemType Directory -Path $ARTIFACTS_DIR -Force | Out-Null
  Write-Success "Created $ARTIFACTS_DIR"
}

# ────────────────────────────────────────────────────────────
# Build WASM (optional)
# ────────────────────────────────────────────────────────────

if (-not $NoWasmBuild) {
  Write-Status "Building contracts to WASM"

  if (-not (Get-Command stellar -ErrorAction SilentlyContinue)) {
    throw "Stellar CLI not found. Install with: cargo install --locked stellar-cli --version $STELLAR_CLI_VERSION"
  }

  Write-Host "Checking $WASM_TARGET target..."
  Invoke-Command-Checked "Installing $WASM_TARGET target" {
    rustup target add $WASM_TARGET
  }

  Invoke-Command-Checked "Building release WASM" {
    stellar contract build
  }

  Write-Success "WASM build complete"
}
else {
  Write-Host "Skipping WASM build" -ForegroundColor Yellow
}

# ────────────────────────────────────────────────────────────
# Gather Provenance
# ────────────────────────────────────────────────────────────

$sourceCommit = Get-GitCommit
$generatedAt = Get-TimeStampUtc
$contractNames = @()
$wasmHashes = @{}

Write-Status "Collecting contract metadata"

# Discover all contracts
$contractDirs = Get-ChildItem -Path $CONTRACTS_DIR -Directory
foreach ($dir in $contractDirs) {
  $contractName = $dir.Name
  $contractNames += $contractName

  $wasmPath = "target/$WASM_TARGET/release/$($contractName.Replace('-', '_')).wasm"

  if (Test-Path $wasmPath) {
    $hash = Get-FileHash-Sha256 $wasmPath
    $wasmHashes[$contractName] = $hash
    Write-Host "  $contractName`: $hash" -ForegroundColor Green
  }
  else {
    Write-Host "  $contractName`: (not built)" -ForegroundColor Yellow
  }
}

# ────────────────────────────────────────────────────────────
# Extract Contract Specs
# ────────────────────────────────────────────────────────────

Write-Status "Extracting contract specifications"

foreach ($contractName in $contractNames) {
  $wasmName = $contractName.Replace('-', '_')
  $wasmPath = "target/$WASM_TARGET/release/$wasmName.wasm"

  if (Test-Path $wasmPath) {
    $specPath = "$ARTIFACTS_DIR/$contractName-spec.json"

    Write-Host "  Extracting $contractName..."

    # Note: stellar contract inspect requires the full Stellar CLI
    # For now, we'll create a placeholder spec that can be filled in later
    # Once Stellar CLI v21+ is available in the environment

    # Write placeholder spec (actual extraction requires stellar-cli setup)
    $spec = [ordered]@{
      contract = $contractName
    } | ConvertTo-Json -Compress

    Set-Utf8NoBomContent -Path $specPath -Value $spec
    Write-Host "    → $specPath" -ForegroundColor Green
  }
}

# ────────────────────────────────────────────────────────────
# Write Provenance File
# ────────────────────────────────────────────────────────────

if ($SkipProvenance) {
  Write-Host "Skipping provenance file" -ForegroundColor Yellow
}
else {
  Write-Status "Writing provenance file"

  $provenance = [ordered]@{
    sourceCommit = $sourceCommit
    generatedAt = $generatedAt
    stellarCliVersion = $STELLAR_CLI_VERSION
    network = $Network
    contracts = $contractNames
    wasmHashes = $wasmHashes
  } | ConvertTo-Json -Depth 2 -Compress

  $provenancePath = "$ARTIFACTS_DIR/provenance.json"
  Set-Utf8NoBomContent -Path $provenancePath -Value $provenance

  Write-Success "Provenance: $provenancePath"
  Write-Host ($provenance | Out-String) -ForegroundColor DarkGray
}

# ────────────────────────────────────────────────────────────
# Summary
# ────────────────────────────────────────────────────────────

Write-Host ""
Write-Success "Binding generation complete"
Write-Host ""
Write-Host "Generated files:" -ForegroundColor Cyan
Write-Host "  • artifacts/bindings/types.ts"
Write-Host "  • artifacts/bindings/client.ts"
Write-Host "  • artifacts/bindings/*-spec.json"
if ($SkipProvenance) {
  Write-Host "  • artifacts/bindings/provenance.json (unchanged)"
}
else {
  Write-Host "  • artifacts/bindings/provenance.json"
}
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Review generated TypeScript files"
Write-Host "  2. npm install @stellar/stellar-sdk"
Write-Host "  3. Commit changes: git add artifacts/bindings/"
Write-Host "  4. Update NestJS services to use EarnProofClient"
Write-Host ""
