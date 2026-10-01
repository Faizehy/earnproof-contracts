# Admin Operations Runbook

## Contracts
Contracts: ProtocolConfig, Issuer, ProofRegistry.
Each contract has an admin address and an `update_admin(new_admin)` function.

## Rotation Interface
- Auth: only current admin can call.
- Rejects: zero address and no-op.
- Emits: `AdminUpdated(old_admin, new_admin)`.

## Order
1. Issuer
2. ProofRegistry
3. ProtocolConfig

Verify after each step.

## Verification
- Query `admin()` on each contract after each rotation.
- Run `./scripts/verify-manifest.ps1` to check the manifest.

## Partial Failure Recovery
- Determine which contracts have already been rotated.
- Retry the remaining ones using the old admin key.
- Complete all rotations before verifying.

## Emergency Key Compromise
- If the current admin key is compromised, immediately rotate all contracts to a fresh securely generated key.
- If the key is lost, use the governed alternative (e.g., time-locked multisig) to reset the admin.
- Audit all configurations after recovery.

## Prevention
- Use a multisig address as the admin to avoid single-key lockout.

## Threshold Approval for Critical Changes

Threshold approval is configured independently on `ProtocolConfig` and
`ProofRegistry`. It is disabled by default. The current admin bootstraps a
policy containing a unique signer list and a threshold from 1 through the
number of signers, up to 16. Once enabled, the policy itself can only be
changed through an approved `ApprovalPolicyUpdate` proposal.

With the policy enabled, proposals are required for:

- `ProtocolConfig`: schema approval, schema deprecation, payload-size limit
	changes, and approval-policy updates.
- `ProofRegistry`: issuer-registry replacement, protocol-config replacement,
	and approval-policy updates.

The admin creates a proposal for the exact action parameters. Each configured
signer separately calls `approve_critical_action` with the returned proposal
ID and signs as that signer. After the threshold is met, anyone may call
`execute_critical_action`. The proposal expires after 518,400 ledgers;
`cancel_critical_action` lets the admin cancel it earlier. Failed validation
does not consume the proposal, so its parameters or target can be reviewed
and the proposal cancelled if execution cannot proceed.

Proposal IDs are scoped to the contract instance and commit to the action
category, parameters, and a monotonic nonce. They cannot authorize a different
action or be replayed after successful execution. Dependency replacements
still undergo the existing interface-version and address checks at execution.

Emergency `pause`, `unpause`, and scoped-pause calls remain immediate admin
operations and are not subject to threshold approval.
