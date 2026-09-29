use crate::harness::Deployment;
use earnproof_shared::ProofError;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::BytesN as _;
use soroban_sdk::{Address, BytesN};

/// Helper: deterministic 32-byte value.
fn hash(env: &soroban_sdk::Env, discriminator: u8) -> BytesN<32> {
    BytesN::from_array(env, &[discriminator; 32])
}

#[test]
fn predecessor_not_found() {
    let deployment = Deployment::new();
    let env = &deployment.env;

    let p1 = BytesN::random(env);
    let p2 = BytesN::random(env);

    // Try to register p2 succeeding p1 (which doesn't exist)
    let commitment = hash(env, 0xCC);
    let expires_at = env.ledger().timestamp() + 100_000;

    let res = deployment.proofs.try_register_proof(
        &p2,
        &commitment,
        &deployment.issuer,
        &crate::harness::APPROVED_SCHEMA,
        &expires_at,
        &Some(p1),
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1; 32]),
    );

    assert_eq!(res.unwrap_err().unwrap(), ProofError::PredecessorNotFound);
}

#[test]
fn cyclic_supersession() {
    let deployment = Deployment::new();
    let env = &deployment.env;

    let p1 = BytesN::random(env);

    // Try to register p1 succeeding itself
    let commitment = hash(env, 0xCC);
    let expires_at = env.ledger().timestamp() + 100_000;

    let res = deployment.proofs.try_register_proof(
        &p1,
        &commitment,
        &deployment.issuer,
        &crate::harness::APPROVED_SCHEMA,
        &expires_at,
        &Some(p1.clone()),
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1; 32]),
    );

    assert_eq!(res.unwrap_err().unwrap(), ProofError::CyclicSupersession);
}

#[test]
fn cross_issuer_supersession() {
    let deployment = Deployment::new();
    let env = &deployment.env;

    let p1 = BytesN::random(env);
    deployment.register_proof(&p1, None);

    // Setup second issuer
    let issuer2 = Address::generate(env);
    let issuer2_id = hash(env, 0x02);
    deployment
        .issuers
        .register_issuer(&issuer2_id, &issuer2, &hash(env, 0xBB), &hash(env, 0x88));

    let p2 = BytesN::random(env);
    let commitment = hash(env, 0xCC);
    let expires_at = env.ledger().timestamp() + 100_000;

    // Try to register p2 by issuer2 succeeding p1 (which was issued by issuer1)
    let res = deployment.proofs.try_register_proof(
        &p2,
        &commitment,
        &issuer2,
        &crate::harness::APPROVED_SCHEMA,
        &expires_at,
        &Some(p1),
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1; 32]),
    );

    assert_eq!(
        res.unwrap_err().unwrap(),
        ProofError::CrossIssuerSupersession
    );
}

#[test]
fn too_many_successors() {
    let deployment = Deployment::new();
    let env = &deployment.env;

    let p1 = BytesN::random(env);
    deployment.register_proof(&p1, None);

    for _ in 0..5 {
        let p = BytesN::random(env);
        deployment.register_proof(&p, Some(p1.clone()));
    }

    let p_fail = BytesN::random(env);
    let commitment = hash(env, 0xCC);
    let expires_at = env.ledger().timestamp() + 100_000;

    let res = deployment.proofs.try_register_proof(
        &p_fail,
        &commitment,
        &deployment.issuer,
        &crate::harness::APPROVED_SCHEMA,
        &expires_at,
        &Some(p1),
        &soroban_sdk::BytesN::from_array(&deployment.env, &[1; 32]),
    );

    assert_eq!(res.unwrap_err().unwrap(), ProofError::TooManySuccessors);
}
