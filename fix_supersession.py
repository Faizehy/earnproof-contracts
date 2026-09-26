import os

def fix_harness():
    with open("tests/supersession/src/harness.rs", "r") as f:
        content = f.read()
        
    content = content.replace("use protocol_config::ProtocolConfigContract;", "use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};\nuse issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};\nuse proof_registry::{ProofRegistryContract, ProofRegistryContractClient};")
    content = content.replace("use issuer_registry::IssuerRegistryContract;", "")
    content = content.replace("use proof_registry::ProofRegistryContract;", "")
    
    with open("tests/supersession/src/harness.rs", "w") as f:
        f.write(content)

def fix_errors():
    with open("tests/supersession/src/errors.rs", "r") as f:
        content = f.read()
        
    content = content.replace("use soroban_sdk::{BytesN, Address, bytes};", "use soroban_sdk::{BytesN, Address, bytes};\nuse soroban_sdk::testutils::Address as _;")
    
    with open("tests/supersession/src/errors.rs", "w") as f:
        f.write(content)

fix_harness()
fix_errors()
print("Fixed supersession test imports.")
