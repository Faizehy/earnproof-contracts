//! Pause behaviour for every public read and mutation.
//!
//! The table below is the executable form of the "Behaviour under pause" section
//! of `docs/emergency-operations.md`. Each entry names one public entry point
//! and records whether it is expected to remain available while the protocol is
//! paused.
//!
//! Every case is driven through the generated `try_*` client methods, which
//! surface a rejected invocation as `Err` instead of unwinding. That lets one
//! table assert both the "available" and "contained" expectations, and lets a
//! scenario keep using the deployment after a rejected call — which is what the
//! ordering and recovery assertions depend on.

use crate::harness::{hash, issuer_id_hash, Deployment, APPROVED_SCHEMA};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Address;

/// Whether an entry point is expected to work while the protocol is paused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WhenPaused {
    /// Remains callable: reads, and the containment operations an operator
    /// needs during an incident.
    Available,
    /// Rejected for as long as the pause is in force.
    Contained,
}

use WhenPaused::{Available, Contained};

/// Outcome of one entry-point invocation, collapsed to success or rejection.
type Outcome = Result<(), ()>;

/// One row of the pause matrix.
struct Case {
    /// Fully-qualified entry point, matching the table in the operations doc.
    name: &'static str,
    expected: WhenPaused,
    /// Fixture state that must exist before the entry point can be exercised.
    ///
    /// Runs while the protocol is still unpaused. Rows that verify reads over a
    /// registered proof need one to exist, and registration is itself contained
    /// by the pause — so the fixture cannot be created inside `call`.
    setup: fn(&Deployment),
    call: fn(&Deployment) -> Outcome,
}

/// Default [`Case::setup`] for rows that need no fixture.
fn no_setup(_: &Deployment) {}

/// [`Case::setup`] for rows that read back a registered proof.
fn register_fixture_proof(d: &Deployment) {
    d.register_proof(FIXTURE_PROOF);
}

/// Discriminator of the proof created by [`register_fixture_proof`].
const FIXTURE_PROOF: u8 = 0x11;

/// Collapses a `try_*` result to [`Outcome`].
///
/// A `try_*` call returns `Result<Result<T, ConversionError>, Result<Error, InvokeError>>`.
/// The outer `Err` is the contract rejecting the call, which is the only
/// distinction the pause matrix cares about.
fn settled<T, C, E>(result: Result<Result<T, C>, E>) -> Outcome {
    match result {
        Ok(_) => Ok(()),
        Err(_) => Err(()),
    }
}

/// Every public entry point across the three contracts.
///
/// A new public function without a row here is a documentation gap:
/// [`matrix_covers_every_public_entry_point`] fails when the count drifts.
fn matrix() -> std::vec::Vec<Case> {
    std::vec![
        // ---- protocol-config: reads -------------------------------------
        Case {
            name: "protocol-config::get_admin",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.config.try_get_admin()),
        },
        Case {
            name: "protocol-config::is_paused",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.config.try_is_paused()),
        },
        Case {
            name: "protocol-config::get_config_version",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.config.try_get_config_version()),
        },
        Case {
            name: "protocol-config::is_schema_version_approved",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.config.try_is_schema_version_approved(&APPROVED_SCHEMA)),
        },
        // ---- protocol-config: mutations ---------------------------------
        // Schema administration stays available: an operator responding to a
        // bad schema must be able to deprecate it without first unpausing,
        // which would re-open proof registration mid-incident.
        Case {
            name: "protocol-config::approve_schema_version",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.config.try_approve_schema_version(&8)),
        },
        Case {
            name: "protocol-config::deprecate_schema_version",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.config.try_deprecate_schema_version(&APPROVED_SCHEMA)),
        },
        Case {
            name: "protocol-config::set_admin",
            expected: Available,
            setup: no_setup,
            call: |d| {
                let next = Address::generate(&d.env);
                {
                    let res = d.config.try_nominate_admin(&next);
                    if res.is_ok() {
                        let _ = d.config.try_accept_admin();
                    }
                    settled(res)
                }
            },
        },
        Case {
            name: "protocol-config::pause",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.config.try_pause()),
        },
        Case {
            name: "protocol-config::unpause",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.config.try_unpause()),
        },
        // ---- issuer-registry: reads -------------------------------------
        Case {
            name: "issuer-registry::get_admin",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.issuers.try_get_admin()),
        },
        Case {
            name: "issuer-registry::get_issuer",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.issuers.try_get_issuer(&issuer_id_hash(&d.env, 1))),
        },
        Case {
            name: "issuer-registry::get_issuer_by_address",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.issuers.try_get_issuer_by_address(&d.issuer)),
        },
        Case {
            name: "issuer-registry::is_active_issuer",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.issuers.try_is_active_issuer(&issuer_id_hash(&d.env, 1))),
        },
        Case {
            name: "issuer-registry::is_active_address",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.issuers.try_is_active_address(&d.issuer)),
        },
        // ---- issuer-registry: mutations ---------------------------------
        // The issuer registry does not consult the pause flag. That is a
        // deliberate containment property, not an oversight: suspending or
        // revoking a compromised issuer is exactly the action an operator needs
        // while the protocol is paused.
        Case {
            name: "issuer-registry::register_issuer",
            expected: Available,
            setup: no_setup,
            call: |d| {
                let next = Address::generate(&d.env);
                settled(d.issuers.try_register_issuer(
                    &issuer_id_hash(&d.env, 2),
                    &next,
                    &hash(&d.env, 0xCC),
                    &hash(&d.env, 0x99),
                ))
            },
        },
        Case {
            name: "issuer-registry::update_issuer",
            expected: Available,
            setup: no_setup,
            call: |d| {
                settled(
                    d.issuers
                        .try_update_issuer(&issuer_id_hash(&d.env, 1), &hash(&d.env, 0xBB)),
                )
            },
        },
        Case {
            name: "issuer-registry::suspend_issuer",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.issuers.try_suspend_issuer(
                &issuer_id_hash(&d.env, 1),
                &soroban_sdk::BytesN::from_array(&d.env, &[1u8; 32])
            )),
        },
        Case {
            name: "issuer-registry::reactivate_issuer",
            expected: Available,
            setup: no_setup,
            call: |d| {
                d.issuers.suspend_issuer(
                    &issuer_id_hash(&d.env, 1),
                    &soroban_sdk::BytesN::from_array(&d.env, &[1u8; 32]),
                );
                settled(d.issuers.try_reactivate_issuer(
                    &issuer_id_hash(&d.env, 1),
                    &soroban_sdk::BytesN::from_array(&d.env, &[1u8; 32]),
                ))
            },
        },
        Case {
            name: "issuer-registry::revoke_issuer",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.issuers.try_revoke_issuer(
                &issuer_id_hash(&d.env, 1),
                &soroban_sdk::BytesN::from_array(&d.env, &[1u8; 32])
            )),
        },
        Case {
            name: "issuer-registry::rotate_issuer_address",
            expected: Available,
            setup: no_setup,
            call: |d| {
                let next = Address::generate(&d.env);
                settled(
                    d.issuers
                        .try_rotate_issuer_address(&issuer_id_hash(&d.env, 1), &next),
                )
            },
        },
        // ---- proof-registry: reads --------------------------------------
        Case {
            name: "proof-registry::get_admin",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.proofs.try_get_admin()),
        },
        Case {
            name: "proof-registry::get_issuer_registry",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.proofs.try_get_issuer_registry()),
        },
        Case {
            name: "proof-registry::get_protocol_config",
            expected: Available,
            setup: no_setup,
            call: |d| settled(d.proofs.try_get_protocol_config()),
        },
        // Verification must keep working while paused. A relying party holding
        // a credential still needs to learn whether it was revoked; a pause that
        // blinded verifiers would turn an incident into a silent outage.
        Case {
            name: "proof-registry::get_proof",
            expected: Available,
            setup: register_fixture_proof,
            call: |d| settled(d.proofs.try_get_proof(&hash(&d.env, FIXTURE_PROOF))),
        },
        Case {
            name: "proof-registry::is_valid_proof",
            expected: Available,
            setup: register_fixture_proof,
            call: |d| settled(d.proofs.try_is_valid_proof(&hash(&d.env, FIXTURE_PROOF))),
        },
        Case {
            name: "proof-registry::is_revoked",
            expected: Available,
            setup: register_fixture_proof,
            call: |d| settled(d.proofs.try_is_revoked(&hash(&d.env, FIXTURE_PROOF))),
        },
        // ---- proof-registry: mutations ----------------------------------
        // Registration is the one entry point the pause switch contains: it is
        // the only operation that admits new obligations during an incident.
        Case {
            name: "proof-registry::register_proof",
            expected: Contained,
            setup: no_setup,
            call: |d| {
                settled(d.proofs.try_register_proof(
                    &hash(&d.env, 0x21),
                    &hash(&d.env, 0x22),
                    &d.issuer,
                    &APPROVED_SCHEMA,
                    &(d.env.ledger().timestamp() + 100_000),
                ))
            },
        },
        // Revocation is a containment operation and stays available by design.
        Case {
            name: "proof-registry::revoke_proof",
            expected: Available,
            setup: register_fixture_proof,
            call: |d| settled(d.proofs.try_revoke_proof(&hash(&d.env, FIXTURE_PROOF))),
        },
        Case {
            name: "proof-registry::admin_revoke_proof",
            expected: Available,
            setup: register_fixture_proof,
            call: |d| settled(
                d.proofs
                    .try_admin_revoke_proof(&hash(&d.env, FIXTURE_PROOF))
            ),
        },
    ]
}

/// Guards against an entry point being added without a documented pause
/// expectation. Bump only together with `docs/emergency-operations.md`.
///
/// `initialize` is excluded from the matrix on all three contracts: it is
/// single-shot and cannot be reached on a deployment that the harness has
/// already initialised. Its rejection is asserted in [`crate::sequences`].
const DOCUMENTED_ENTRY_POINTS: usize = 29;

#[test]
fn matrix_covers_every_public_entry_point() {
    assert_eq!(
        matrix().len(),
        DOCUMENTED_ENTRY_POINTS,
        "pause matrix and docs/emergency-operations.md disagree on the public surface"
    );
}

#[test]
fn every_entry_point_matches_its_documented_pause_behaviour() {
    for case in matrix() {
        // A fresh deployment per case: the matrix mutates state, and a shared
        // deployment would let an earlier row mask a later one.
        let deployment = Deployment::new();
        // Fixtures are built before the pause: some of them depend on the very
        // operation the pause contains.
        (case.setup)(&deployment);
        deployment.config.pause();
        assert!(deployment.config.is_paused());

        let outcome = (case.call)(&deployment);

        match case.expected {
            Available => assert!(
                outcome.is_ok(),
                "{} is documented as available while paused but was rejected",
                case.name
            ),
            Contained => assert!(
                outcome.is_err(),
                "{} is documented as contained while paused but succeeded",
                case.name
            ),
        }
    }
}

#[test]
fn every_entry_point_is_reachable_while_unpaused() {
    // The mirror of the matrix: no row may be "contained" simply because it was
    // already broken. Each case must succeed on an unpaused deployment, so a
    // `Contained` verdict above is attributable to the pause and nothing else.
    for case in matrix() {
        let deployment = Deployment::new();
        (case.setup)(&deployment);
        assert!(!deployment.config.is_paused());

        assert!(
            (case.call)(&deployment).is_ok(),
            "{} failed on an unpaused deployment; its pause verdict is not attributable",
            case.name
        );
    }
}

#[test]
fn reads_return_identical_values_paused_and_unpaused() {
    let deployment = Deployment::new();
    let proof_id = deployment.register_proof(0x41);

    let before_admin = deployment.proofs.get_admin();
    let before_valid = deployment.proofs.is_valid_proof(&proof_id);
    let before_revoked = deployment.proofs.is_revoked(&proof_id);
    let before_record = deployment.proofs.get_proof(&proof_id);

    deployment.config.pause();

    assert_eq!(deployment.proofs.get_admin(), before_admin);
    assert_eq!(deployment.proofs.is_valid_proof(&proof_id), before_valid);
    assert_eq!(deployment.proofs.is_revoked(&proof_id), before_revoked);
    assert_eq!(deployment.proofs.get_proof(&proof_id), before_record);
}

#[test]
fn containment_survives_repeated_pause_calls() {
    let deployment = Deployment::new();

    // Pausing an already-paused protocol must be idempotent for containment,
    // not a toggle. An operator hitting the button twice under pressure must
    // not re-open registration.
    for _ in 0..3 {
        deployment.config.pause();
        assert!(deployment.config.is_paused());
    }

    assert!(
        deployment
            .proofs
            .try_register_proof(
                &hash(&deployment.env, 0x51),
                &hash(&deployment.env, 0x52),
                &deployment.issuer,
                &APPROVED_SCHEMA,
                &(deployment.env.ledger().timestamp() + 100_000),
            )
            .is_err(),
        "repeated pause must keep registration contained"
    );
}

#[test]
fn unpause_restores_exactly_the_contained_operation() {
    let deployment = Deployment::new();
    deployment.config.pause();
    deployment.config.unpause();
    assert!(!deployment.config.is_paused());

    // The single contained operation comes back; the harness call panics if not.
    deployment.register_proof(0x61);
}

#[test]
fn revocation_of_an_expired_proof_still_records_revocation() {
    // An incident often outlives the proofs it touches. Revocation evidence must
    // remain recordable so a later audit can distinguish "expired" from
    // "revoked during the incident".
    let deployment = Deployment::new();
    let proof_id = deployment.register_proof(0x71);

    deployment.config.pause();
    deployment.advance(200_000);
    assert!(!deployment.proofs.is_valid_proof(&proof_id));

    deployment.proofs.admin_revoke_proof(&proof_id);
    assert!(deployment.proofs.is_revoked(&proof_id));
    assert_ne!(deployment.proofs.get_proof(&proof_id).revoked_at, 0);
}

#[test]
fn double_revocation_is_rejected_without_erasing_the_first() {
    let deployment = Deployment::new();
    let proof_id = deployment.register_proof(0x81);

    deployment.config.pause();
    deployment.proofs.admin_revoke_proof(&proof_id);
    let first = deployment.proofs.get_proof(&proof_id);

    assert!(
        deployment.proofs.try_admin_revoke_proof(&proof_id).is_err(),
        "a second revocation must be rejected"
    );

    // The rejected call must not have moved the revocation timestamp, which is
    // the evidence an auditor uses to reconstruct the incident window.
    assert_eq!(deployment.proofs.get_proof(&proof_id), first);
}
