import os

# 1. Create Cargo.toml
cargo_toml = """[package]
name = "supersession-tests"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
publish = false

[lib]
doctest = false

[dependencies]
earnproof-shared.workspace = true
soroban-sdk.workspace = true

[dev-dependencies]
issuer-registry = { path = "../../contracts/issuer-registry" }
proof-registry = { path = "../../contracts/proof-registry" }
protocol-config = { path = "../../contracts/protocol-config" }
soroban-sdk = { workspace = true, features = ["testutils"] }
"""

with open("tests/supersession/Cargo.toml", "w") as f:
    f.write(cargo_toml)

# 2. Add to root Cargo.toml
with open("Cargo.toml", "r") as f:
    root_cargo = f.read()
if '"tests/supersession"' not in root_cargo:
    root_cargo = root_cargo.replace('"tests/address-validation",', '"tests/address-validation",\n  "tests/supersession",')
    with open("Cargo.toml", "w") as f:
        f.write(root_cargo)

# 3. Create src/lib.rs
lib_rs = """#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod harness;

#[cfg(test)]
mod chains;

#[cfg(test)]
mod errors;
"""
with open("tests/supersession/src/lib.rs", "w") as f:
    f.write(lib_rs)

# 4. Create src/harness.rs (copied and adapted from authorization)
harness_rs = """use earnproof_shared::ProofError;
use issuer_registry::IssuerRegistryContract;
use proof_registry::ProofRegistryContract;
use protocol_config::ProtocolConfigContract;
use soroban_sdk::{
    bytes,
    testutils::{Address as _, BytesN as _, Ledger},
    Address, BytesN, Env,
};

pub const APPROVED_SCHEMA: u32 = 1;

pub struct Deployment {
    pub env: Env,
    pub config: ProtocolConfigContractClient<'static>,
    pub issuers: IssuerRegistryContractClient<'static>,
    pub proofs: ProofRegistryContractClient<'static>,
    pub issuer: Address,
}

impl Deployment {
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let config_id = env.register_contract(None, ProtocolConfigContract);
        let config = ProtocolConfigContractClient::new(&env, &config_id);
        config.initialize();
        config.approve_schema(&APPROVED_SCHEMA);

        let issuers_id = env.register_contract(None, IssuerRegistryContract);
        let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
        issuers.initialize(&config.address);

        let issuer = Address::generate(&env);
        issuers.register_issuer(&issuer);

        let proofs_id = env.register_contract(None, ProofRegistryContract);
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
        proofs.initialize(&config.address, &issuers.address);

        Self {
            env,
            config,
            issuers,
            proofs,
            issuer,
        }
    }

    pub fn register_proof(
        &self,
        proof_id: &BytesN<32>,
        predecessor: Option<BytesN<32>>,
    ) -> BytesN<32> {
        let commitment = bytes!(&self.env, 0x11);
        let expires_at = self.env.ledger().timestamp() + 100_000;
        self.proofs.register_proof(
            proof_id,
            &commitment,
            &self.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
            &predecessor,
        );
        proof_id.clone()
    }
}
"""
with open("tests/supersession/src/harness.rs", "w") as f:
    f.write(harness_rs)

# 5. Create src/chains.rs
chains_rs = """use crate::harness::Deployment;
use soroban_sdk::testutils::BytesN as _;
use soroban_sdk::{BytesN, vec};

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
"""
with open("tests/supersession/src/chains.rs", "w") as f:
    f.write(chains_rs)

# 6. Create src/errors.rs
errors_rs = """use crate::harness::Deployment;
use earnproof_shared::ProofError;
use soroban_sdk::testutils::BytesN as _;
use soroban_sdk::{BytesN, Address, bytes};

#[test]
fn predecessor_not_found() {
    let deployment = Deployment::new();
    let env = &deployment.env;

    let p1 = BytesN::random(env);
    let p2 = BytesN::random(env);

    // Try to register p2 succeeding p1 (which doesn't exist)
    let commitment = bytes!(env, 0x11);
    let expires_at = env.ledger().timestamp() + 100_000;
    
    let res = deployment.proofs.try_register_proof(
        &p2,
        &commitment,
        &deployment.issuer,
        &crate::harness::APPROVED_SCHEMA,
        &expires_at,
        &Some(p1),
    );
    
    assert_eq!(res.unwrap_err().unwrap(), ProofError::PredecessorNotFound);
}

#[test]
fn cyclic_supersession() {
    let deployment = Deployment::new();
    let env = &deployment.env;

    let p1 = BytesN::random(env);

    // Try to register p1 succeeding itself
    let commitment = bytes!(env, 0x11);
    let expires_at = env.ledger().timestamp() + 100_000;
    
    let res = deployment.proofs.try_register_proof(
        &p1,
        &commitment,
        &deployment.issuer,
        &crate::harness::APPROVED_SCHEMA,
        &expires_at,
        &Some(p1.clone()),
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
    deployment.issuers.register_issuer(&issuer2);

    let p2 = BytesN::random(env);
    let commitment = bytes!(env, 0x11);
    let expires_at = env.ledger().timestamp() + 100_000;
    
    // Try to register p2 by issuer2 succeeding p1 (which was issued by issuer1)
    let res = deployment.proofs.try_register_proof(
        &p2,
        &commitment,
        &issuer2,
        &crate::harness::APPROVED_SCHEMA,
        &expires_at,
        &Some(p1),
    );
    
    assert_eq!(res.unwrap_err().unwrap(), ProofError::CrossIssuerSupersession);
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
    let commitment = bytes!(env, 0x11);
    let expires_at = env.ledger().timestamp() + 100_000;
    
    let res = deployment.proofs.try_register_proof(
        &p_fail,
        &commitment,
        &deployment.issuer,
        &crate::harness::APPROVED_SCHEMA,
        &expires_at,
        &Some(p1),
    );
    
    assert_eq!(res.unwrap_err().unwrap(), ProofError::TooManySuccessors);
}
"""
with open("tests/supersession/src/errors.rs", "w") as f:
    f.write(errors_rs)

print("Created supersession crate.")
