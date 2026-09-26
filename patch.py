import re
import os

MAX_SUCCESSORS = 5

def patch_shared_lib():
    with open("packages/shared/src/lib.rs", "r") as f:
        content = f.read()

    # Add new errors
    error_str = """    MalformedInput = 310,
    CyclicSupersession = 311,
    CrossIssuerSupersession = 312,
    PredecessorNotFound = 313,
    TooManySuccessors = 314,"""
    content = re.sub(r'    MalformedInput = 310,', error_str, content)

    # Add predecessor_id_hash to ProofRecord
    record_str = """    pub created_at: u64,
    pub revoked_at: u64,
    pub predecessor_id_hash: Option<BytesN<32>>,
}"""
    content = re.sub(r'    pub created_at: u64,\s*pub revoked_at: u64,\s*\}', record_str, content)

    # Add constants for MAX_SUCCESSORS
    const_str = """
pub const MAX_SUCCESSORS: u32 = 5;
"""
    content = content + const_str

    with open("packages/shared/src/lib.rs", "w") as f:
        f.write(content)

def patch_error_catalog():
    with open("packages/shared/src/error_catalog.rs", "r") as f:
        content = f.read()

    # Add new ErrorSpecs
    error_specs = """        http_status: 400,
        client_message: "Malformed proof input",
    },
    ErrorSpec {
        code: 311,
        name: "CyclicSupersession",
        enum_name: "ProofError",
        domain: Domain::ProofRegistry,
        status: Status::Returned,
        cause: "The proof identifier matches its own predecessor.",
        retry: Retry::Never,
        remediation: "A proof cannot supersede itself.",
        http_status: 400,
        client_message: "Cyclic supersession detected",
    },
    ErrorSpec {
        code: 312,
        name: "CrossIssuerSupersession",
        enum_name: "ProofError",
        domain: Domain::ProofRegistry,
        status: Status::Returned,
        cause: "The predecessor proof was registered by a different issuer.",
        retry: Retry::Never,
        remediation: "Cross-issuer supersession is rejected.",
        http_status: 403,
        client_message: "Cross-issuer supersession rejected",
    },
    ErrorSpec {
        code: 313,
        name: "PredecessorNotFound",
        enum_name: "ProofError",
        domain: Domain::ProofRegistry,
        status: Status::Returned,
        cause: "The specified predecessor proof was not found.",
        retry: Retry::AfterCallerChange,
        remediation: "Ensure the predecessor proof exists.",
        http_status: 404,
        client_message: "Predecessor proof not found",
    },
    ErrorSpec {
        code: 314,
        name: "TooManySuccessors",
        enum_name: "ProofError",
        domain: Domain::ProofRegistry,
        status: Status::Returned,
        cause: "The predecessor proof already has the maximum number of successors.",
        retry: Retry::Never,
        remediation: "A predecessor can only have a bounded number of successors.",
        http_status: 400,
        client_message: "Too many successors",
    },
];"""
    content = re.sub(r'        http_status: 400,\s*client_message: "Malformed proof input",\s*\},\s*\];', error_specs, content)

    # Change array size from 25 to 29
    content = re.sub(r'pub const ERROR_CATALOG: \[ErrorSpec; 25\] = \[', 'pub const ERROR_CATALOG: [ErrorSpec; 29] = [', content)

    with open("packages/shared/src/error_catalog.rs", "w") as f:
        f.write(content)

def patch_proof_registry():
    with open("contracts/proof-registry/src/lib.rs", "r") as f:
        content = f.read()

    # Add DataKey::Successors
    data_key_str = """    ContractVersion,
    Successor,
    Decommissioned,
    Successors(BytesN<32>),
}"""
    content = re.sub(r'    ContractVersion,\s*Successor,\s*Decommissioned,\s*\}', data_key_str, content)

    # Update register_proof signature and logic
    register_sig = """    pub fn register_proof(
        env: Env,
        proof_id_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
        issuer_address: Address,
        schema_version: u32,
        expires_at: u64,
        predecessor_id_hash: Option<BytesN<32>>,
    ) -> Result<(), ProofError> {"""
    content = re.sub(r'    pub fn register_proof\(\s*env: Env,\s*proof_id_hash: BytesN<32>,\s*commitment_hash: BytesN<32>,\s*issuer_address: Address,\s*schema_version: u32,\s*expires_at: u64,\s*\) -> Result<\(\), ProofError> \{', register_sig, content)

    # Add supersession validation
    val_str = """        // Check 5: Uniqueness constraint (storage precondition)
        let key = DataKey::Proof(proof_id_hash.clone());
        if env.storage().persistent().has(&key) {
            return Err(ProofError::ProofAlreadyRegistered);
        }

        if let Some(pred_id) = &predecessor_id_hash {
            if pred_id == &proof_id_hash {
                return Err(ProofError::CyclicSupersession);
            }
            let pred_key = DataKey::Proof(pred_id.clone());
            let pred_record: ProofRecord = env
                .storage()
                .persistent()
                .get(&pred_key)
                .ok_or(ProofError::PredecessorNotFound)?;
            if pred_record.issuer_address != issuer_address {
                return Err(ProofError::CrossIssuerSupersession);
            }
            
            let successors_key = DataKey::Successors(pred_id.clone());
            let mut successors: soroban_sdk::Vec<BytesN<32>> = env
                .storage()
                .persistent()
                .get(&successors_key)
                .unwrap_or_else(|| soroban_sdk::vec![&env]);
            
            if successors.len() >= earnproof_shared::MAX_SUCCESSORS {
                return Err(ProofError::TooManySuccessors);
            }
            successors.push_back(proof_id_hash.clone());
            env.storage().persistent().set(&successors_key, &successors);
            Self::extend_proof_key_ttl(env.clone(), &successors_key);
        }"""
    content = re.sub(r'        // Check 5: Uniqueness constraint \(storage precondition\)\s*let key = DataKey::Proof\(proof_id_hash\.clone\(\)\);\s*if env\.storage\(\)\.persistent\(\)\.has\(&key\) \{\s*return Err\(ProofError::ProofAlreadyRegistered\);\s*\}', val_str, content)

    record_creation = """        let record = ProofRecord {
            proof_id_hash,
            commitment_hash,
            issuer_address,
            status: ProofStatus::Active,
            schema_version,
            expires_at,
            created_at: now,
            revoked_at: 0,
            predecessor_id_hash,
        };"""
    content = re.sub(r'        let record = ProofRecord \{\s*proof_id_hash,\s*commitment_hash,\s*issuer_address,\s*status: ProofStatus::Active,\s*schema_version,\s*expires_at,\s*created_at: now,\s*revoked_at: 0,\s*\};', record_creation, content)

    # Add get_successors method
    get_succ_str = """
    pub fn get_successors(env: Env, proof_id_hash: BytesN<32>) -> soroban_sdk::Vec<BytesN<32>> {
        let key = DataKey::Successors(proof_id_hash);
        let successors: soroban_sdk::Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::vec![&env]);
        if env.storage().persistent().has(&key) {
            Self::extend_proof_key_ttl(env, &key);
        }
        successors
    }

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {"""
    content = re.sub(r'    pub fn get_admin\(env: Env\) -> Result<Address, ContractError> \{', get_succ_str, content)

    with open("contracts/proof-registry/src/lib.rs", "w") as f:
        f.write(content)

def patch_storage_namespaces():
    with open("packages/shared/src/storage_namespaces.rs", "r") as f:
        content = f.read()

    # Successors namespace
    # Proof registry is 300, so maybe 305?
    ns_str = """    ProofRecord = 304,
    /// Forward supersession lookup
    ProofSuccessors = 305,
}"""
    content = re.sub(r'    ProofRecord = 304,\s*\}', ns_str, content)
    with open("packages/shared/src/storage_namespaces.rs", "w") as f:
        f.write(content)

if __name__ == "__main__":
    patch_shared_lib()
    patch_error_catalog()
    patch_proof_registry()
    patch_storage_namespaces()
    print("Patched basic contract files.")
