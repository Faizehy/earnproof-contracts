use crate::harness::Deployment;
use soroban_sdk::testutils::BytesN as _;
use soroban_sdk::{vec, BytesN};

#[test]
fn basic_chain() {
    let deployment = Deployment::new();
    let env = &deployment.env;

    let p1 = BytesN::random(env);
    let p2 = BytesN::random(env);
    let p3 = BytesN::random(env);

    // Register p1
    deployment.register_proof(&p1, None);

    // Register p2 succeeding p1
    deployment.register_proof(&p2, Some(p1.clone()));

    // Register p3 succeeding p2
    deployment.register_proof(&p3, Some(p2.clone()));

    // Verify successors of p1
    let successors_p1 = deployment.proofs.get_successors(&p1);
    assert_eq!(successors_p1, vec![env, p2.clone()]);

    // Verify successors of p2
    let successors_p2 = deployment.proofs.get_successors(&p2);
    assert_eq!(successors_p2, vec![env, p3.clone()]);
}

#[test]
fn forks() {
    let deployment = Deployment::new();
    let env = &deployment.env;

    let p1 = BytesN::random(env);
    let p2 = BytesN::random(env);
    let p3 = BytesN::random(env);

    // Register p1
    deployment.register_proof(&p1, None);

    // Fork: Register p2 succeeding p1
    deployment.register_proof(&p2, Some(p1.clone()));

    // Fork: Register p3 succeeding p1
    deployment.register_proof(&p3, Some(p1.clone()));

    // Verify successors of p1 has both p2 and p3
    let successors_p1 = deployment.proofs.get_successors(&p1);
    assert_eq!(successors_p1.len(), 2);
    assert!(successors_p1.contains(&p2) || successors_p1.contains(&p3));
}
