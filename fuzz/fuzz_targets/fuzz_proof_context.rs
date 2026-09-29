#![no_main]

use earnproof_shared::{compute_proof_context_commitments, ProofAssetIdentifier};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Ledger as _, Address, BytesN, Env, String};

const ISSUER: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 {
        return;
    }

    let env = Env::default();
    let split = data.len() / 2;
    let passphrase_text = std::string::String::from_utf8_lossy(&data[..split]);
    let asset_code_text = std::string::String::from_utf8_lossy(&data[split..]);
    let passphrase = String::from_str(&env, &passphrase_text);
    let network_id = env.crypto().sha256(&passphrase.to_bytes()).to_array();
    env.ledger().set_network_id(network_id);

    let asset = if data.first().copied().unwrap_or_default() & 1 == 0 {
        ProofAssetIdentifier::Native
    } else {
        ProofAssetIdentifier::Issued(
            String::from_str(&env, &asset_code_text),
            Address::from_str(&env, ISSUER),
        )
    };
    let claim = BytesN::from_array(&env, &[0x5a; 32]);

    let _ = compute_proof_context_commitments(&env, &claim, &passphrase, &asset);
});
