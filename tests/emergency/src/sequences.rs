//! Generated operation sequences.
//!
//! The matrix in [`crate::pause_matrix`] checks one operation at a time against
//! a known state. This module checks the *orderings*: every permutation of a
//! small emergency alphabet is replayed against a fresh deployment, and the
//! resulting state is compared against an independent model of what the
//! documented rules say should have happened.
//!
//! The generator is exhaustive rather than random. The alphabet is small enough
//! that every ordering up to a bounded length can be enumerated, which gives a
//! deterministic, reproducible suite — a failing case names an exact sequence
//! rather than a seed.

use crate::harness::{hash, issuer_id_hash, Deployment, APPROVED_SCHEMA};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Address;

/// One step an operator can take during an incident.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Pause,
    Unpause,
    RotateAdmin,
    SuspendIssuer,
    ReactivateIssuer,
    RevokeIssuer,
    RevokeProof,
    RegisterProof,
}

use Op::{
    Pause, ReactivateIssuer, RegisterProof, RevokeIssuer, RevokeProof, RotateAdmin, SuspendIssuer,
    Unpause,
};

/// The alphabet replayed by [`every_ordering_matches_the_model`].
const ALPHABET: [Op; 8] = [
    Pause,
    Unpause,
    RotateAdmin,
    SuspendIssuer,
    ReactivateIssuer,
    RevokeIssuer,
    RevokeProof,
    RegisterProof,
];

/// Independent model of the documented emergency rules.
///
/// This is deliberately written from `docs/emergency-operations.md` rather than
/// from the contract source: a model that mirrored the implementation would
/// agree with it even when both are wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Model {
    paused: bool,
    issuer_suspended: bool,
    issuer_revoked: bool,
    proof_revoked: bool,
    /// Whether the fixture proof exists at all.
    proof_registered: bool,
}

impl Model {
    fn new() -> Self {
        Self {
            paused: false,
            issuer_suspended: false,
            issuer_revoked: false,
            proof_revoked: false,
            proof_registered: true,
        }
    }

    /// Applies `op`, returning `false` when the documented rules say the call
    /// is rejected. A rejected call must leave the model untouched, mirroring
    /// the contract's all-or-nothing invocation semantics.
    fn apply(&mut self, op: Op) -> bool {
        match op {
            // Pause state is idempotent in both directions and always available
            // to the current administrator.
            Pause => {
                self.paused = true;
                true
            }
            Unpause => {
                self.paused = false;
                true
            }
            // Rotation changes authority only. It never touches containment.
            RotateAdmin => true,
            // Issuer status transitions are admin-gated but pause-independent.
            // Revocation is terminal.
            SuspendIssuer => {
                if self.issuer_revoked {
                    return false;
                }
                self.issuer_suspended = true;
                true
            }
            ReactivateIssuer => {
                if self.issuer_revoked {
                    return false;
                }
                self.issuer_suspended = false;
                true
            }
            RevokeIssuer => {
                self.issuer_revoked = true;
                self.issuer_suspended = false;
                true
            }
            // Revocation is a containment operation: available while paused,
            // but only once, and only for a proof that exists.
            RevokeProof => {
                if !self.proof_registered || self.proof_revoked {
                    return false;
                }
                self.proof_revoked = true;
                true
            }
            // Registration is the one operation the pause contains, and it also
            // requires an active issuer.
            RegisterProof => !self.paused && !self.issuer_suspended && !self.issuer_revoked,
        }
    }
}

/// Applies `op` to a live deployment, returning `false` when it was rejected.
///
/// `RegisterProof` uses a fresh discriminator per step so that a repeat within
/// one sequence is not rejected merely for colliding with its own earlier
/// registration — the property under test is the pause and issuer gating, not
/// duplicate-key handling, which [`crate::pause_matrix`] covers separately.
fn apply_to_contracts(deployment: &Deployment, op: Op, step: usize) -> bool {
    let issuer_id = issuer_id_hash(&deployment.env, 1);
    let fixture_proof = hash(&deployment.env, FIXTURE_PROOF);

    match op {
        Pause => deployment.config.try_pause().is_ok(),
        Unpause => deployment.config.try_unpause().is_ok(),
        RotateAdmin => {
            let next = Address::generate(&deployment.env);
            {
                let r = deployment.config.try_nominate_admin(&next);
                if r.is_ok() {
                    let _ = deployment.config.try_accept_admin();
                }
                r.is_ok()
            }
        }
        SuspendIssuer => deployment
            .issuers
            .try_suspend_issuer(
                &issuer_id,
                &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
            )
            .is_ok(),
        ReactivateIssuer => deployment
            .issuers
            .try_reactivate_issuer(
                &issuer_id,
                &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
            )
            .is_ok(),
        RevokeIssuer => deployment
            .issuers
            .try_revoke_issuer(
                &issuer_id,
                &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
            )
            .is_ok(),
        RevokeProof => deployment
            .proofs
            .try_admin_revoke_proof(&fixture_proof)
            .is_ok(),
        RegisterProof => {
            let discriminator = 0x40u8.wrapping_add(step as u8);
            deployment
                .proofs
                .try_register_proof(
                    &hash(&deployment.env, discriminator),
                    &hash(&deployment.env, discriminator ^ 0xFF),
                    &deployment.issuer,
                    &APPROVED_SCHEMA,
                    &(deployment.env.ledger().timestamp() + 100_000),
                )
                .is_ok()
        }
    }
}

/// Discriminator of the proof every sequence starts with.
const FIXTURE_PROOF: u8 = 0xD1;

/// Reads the observable state back out of the deployment.
fn observe(deployment: &Deployment) -> Model {
    let issuer_id = issuer_id_hash(&deployment.env, 1);
    let record = deployment.issuers.get_issuer(&issuer_id);
    let fixture_proof = hash(&deployment.env, FIXTURE_PROOF);

    Model {
        paused: deployment.config.is_paused(),
        issuer_suspended: record.status == earnproof_shared::IssuerStatus::Suspended,
        issuer_revoked: record.status == earnproof_shared::IssuerStatus::Revoked,
        proof_revoked: deployment.proofs.is_revoked(&fixture_proof),
        proof_registered: true,
    }
}

/// Builds a deployment whose fixture proof is already registered.
fn deployment_with_fixture() -> Deployment<'static> {
    let deployment = Deployment::new();
    deployment.register_proof(FIXTURE_PROOF);
    deployment
}

/// Replays one sequence and asserts the deployment agrees with the model at
/// every step, not just at the end. Checking intermediate states is what makes
/// a reordering failure attributable to the step that caused it.
fn replay(sequence: &[Op]) {
    let deployment = deployment_with_fixture();
    let mut model = Model::new();

    for (step, op) in sequence.iter().enumerate() {
        let mut expected = model;
        let model_accepted = expected.apply(*op);
        let contract_accepted = apply_to_contracts(&deployment, *op, step);

        assert_eq!(
            contract_accepted, model_accepted,
            "step {step} ({op:?}) of {sequence:?}: contract and documented rules disagree on acceptance"
        );

        if model_accepted {
            model = expected;
        }

        assert_eq!(
            observe(&deployment),
            model,
            "step {step} ({op:?}) of {sequence:?}: state diverged from the documented rules"
        );
    }
}

#[test]
fn every_ordering_of_two_operations_matches_the_model() {
    for &first in ALPHABET.iter() {
        for &second in ALPHABET.iter() {
            replay(&[first, second]);
        }
    }
}

#[test]
fn every_ordering_of_three_operations_matches_the_model() {
    for &first in ALPHABET.iter() {
        for &second in ALPHABET.iter() {
            for &third in ALPHABET.iter() {
                replay(&[first, second, third]);
            }
        }
    }
}

#[test]
fn repeating_any_single_operation_is_stable() {
    // Operators retry. Applying the same step four times must converge to the
    // same state the model predicts, with no step flipping a flag back.
    for &op in ALPHABET.iter() {
        replay(&[op, op, op, op]);
    }
}

#[test]
fn conflicting_pairs_resolve_to_the_last_writer() {
    // The pairs below are the ones an operator can plausibly issue in either
    // order during a handover. Whichever lands last must win, with no residue
    // from the loser.
    let conflicts = [
        (Pause, Unpause),
        (SuspendIssuer, ReactivateIssuer),
        (RevokeIssuer, ReactivateIssuer),
        (RevokeProof, RegisterProof),
    ];

    for (a, b) in conflicts {
        replay(&[a, b, a, b]);
        replay(&[b, a, b, a]);
    }
}

#[test]
fn a_paused_protocol_cannot_be_left_without_an_administrator() {
    // The stranding scenario: rotate repeatedly while paused, then confirm the
    // contract still names a reachable administrator and can still be unpaused.
    let deployment = deployment_with_fixture();
    deployment.config.pause();

    let mut current = deployment.admin.clone();
    for _ in 0..5 {
        let next = Address::generate(&deployment.env);
        deployment.config.nominate_admin(&next);
        deployment.config.accept_admin();

        let observed = deployment.config.get_admin();
        assert_eq!(observed, next, "rotation must name the intended successor");
        assert_ne!(observed, current, "rotation must actually move authority");
        current = next;
    }

    assert!(deployment.config.is_paused());
    deployment.config.unpause();
    assert!(
        !deployment.config.is_paused(),
        "a rotated, paused contract must remain recoverable"
    );
}

#[test]
fn initialize_is_rejected_on_an_already_initialized_deployment() {
    // Re-initialisation would reset the administrator without any rotation
    // event, which is the quietest possible privilege escalation.
    let deployment = deployment_with_fixture();
    let attacker = Address::generate(&deployment.env);

    deployment.config.pause();

    assert!(deployment.config.try_initialize(&attacker).is_err());
    assert!(deployment.issuers.try_initialize(&attacker).is_err());
    assert!(deployment
        .proofs
        .try_initialize(
            &attacker,
            &deployment.issuers.address,
            &deployment.config.address
        )
        .is_err());

    assert_eq!(deployment.config.get_admin(), deployment.admin);
    assert_eq!(deployment.issuers.get_admin(), deployment.admin);
    assert_eq!(deployment.proofs.get_admin(), deployment.admin);
    assert!(deployment.config.is_paused());
}

#[test]
fn a_stale_caller_cannot_register_against_a_deprecated_schema() {
    // A caller holding a transaction built before the incident retries after
    // the pause lifts. If the schema it names was deprecated during the
    // incident, the retry must fail — otherwise deprecation would only apply to
    // callers who noticed it.
    let deployment = deployment_with_fixture();

    deployment.config.pause();
    deployment.config.deprecate_schema_version(&APPROVED_SCHEMA);
    deployment.config.unpause();

    assert!(
        deployment
            .proofs
            .try_register_proof(
                &hash(&deployment.env, 0xE1),
                &hash(&deployment.env, 0xE2),
                &deployment.issuer,
                &APPROVED_SCHEMA,
                &(deployment.env.ledger().timestamp() + 100_000),
            )
            .is_err(),
        "a deprecated schema must not be usable after the pause lifts"
    );
}

#[test]
fn cross_contract_disagreement_resolves_in_favour_of_containment() {
    // protocol-config and issuer-registry can disagree: the protocol is
    // unpaused while the issuer is revoked, or paused while the issuer is
    // active. Registration must require *both* to allow it.
    let cases = [
        (false, false, true), // unpaused, issuer active -> allowed
        (true, false, false), // paused -> contained
        (false, true, false), // issuer revoked -> contained
        (true, true, false),  // both -> contained
    ];

    for (paused, revoke_issuer, expected) in cases {
        let deployment = deployment_with_fixture();
        let issuer_id = issuer_id_hash(&deployment.env, 1);

        if revoke_issuer {
            deployment.issuers.revoke_issuer(
                &issuer_id,
                &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
            );
        }
        if paused {
            deployment.config.pause();
        }

        let accepted = deployment
            .proofs
            .try_register_proof(
                &hash(&deployment.env, 0xF1),
                &hash(&deployment.env, 0xF2),
                &deployment.issuer,
                &APPROVED_SCHEMA,
                &(deployment.env.ledger().timestamp() + 100_000),
            )
            .is_ok();

        assert_eq!(
            accepted, expected,
            "paused={paused} issuer_revoked={revoke_issuer}: registration gating is wrong"
        );
    }
}

#[test]
fn rejected_operations_leave_no_partial_state() {
    // Every rejected call in the alphabet must be a no-op. A partial write would
    // let a failed operator action silently change the incident's shape.
    let deployment = deployment_with_fixture();

    // Drive into a state where several operations are rejected: the issuer is
    // revoked (blocking suspend/reactivate) and the fixture proof is revoked
    // (blocking a second revocation).
    deployment.config.pause();
    deployment.issuers.revoke_issuer(
        &issuer_id_hash(&deployment.env, 1),
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1u8; 32]),
    );
    deployment
        .proofs
        .admin_revoke_proof(&hash(&deployment.env, FIXTURE_PROOF));

    let before = observe(&deployment);
    let config_version_before = deployment.config.get_config_version();

    for (step, op) in [SuspendIssuer, ReactivateIssuer, RevokeProof, RegisterProof]
        .iter()
        .enumerate()
    {
        assert!(
            !apply_to_contracts(&deployment, *op, step),
            "{op:?} should be rejected in this state"
        );
        assert_eq!(
            observe(&deployment),
            before,
            "{op:?} was rejected but still changed observable state"
        );
    }

    assert_eq!(
        deployment.config.get_config_version(),
        config_version_before,
        "rejected operations must not advance the config version"
    );
}
