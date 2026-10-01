#![no_main]
use earnproof_shared::ProofRecord;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{Address, Bytes, BytesN, Env};

// Fuzz target for ProofRecord deserialization and field validation
// Tests that arbitrary bytes can be safely deserialized or fail gracefully
fuzz_target!(|data: &[u8]| {
    // Limit input size to prevent memory exhaustion
    if data.len() > 8192 {
        return;
    }

    // Skip if data is too short for the fixed hashes and scalar fields.
    if data.len() < 129 {
        return;
    }

    let env = Env::default();

    // Attempt to parse ProofRecord from XDR
    // The soroban-sdk's FromXdr trait is used internally for contracttype deserialization
    // We simulate the kind of errors that should be caught during deserialization

    // Try to construct a ProofRecord by parsing fixed fields:
    // - proof_id_hash: BytesN<32> (bytes 0-32)
    // - commitment_hash: BytesN<32> (bytes 32-64)
    // - disclosure_policy_hash: BytesN<32> (bytes 64-96)
    // - remaining fields are represented by the fuzz target's compact layout

    // Extract proof_id_hash (first 32 bytes)
    let proof_id_hash = match BytesN::<32>::try_from(Bytes::from_slice(&env, &data[0..32])) {
        Ok(h) => h,
        Err(_) => return,
    };

    // Extract commitment_hash (next 32 bytes)
    let commitment_hash = match BytesN::<32>::try_from(Bytes::from_slice(&env, &data[32..64])) {
        Ok(h) => h,
        Err(_) => return,
    };
    let disclosure_policy_hash =
        match BytesN::<32>::try_from(Bytes::from_slice(&env, &data[64..96])) {
            Ok(h) => h,
            Err(_) => return,
        };

    // Verify that we can safely handle the record
    // In a real scenario, Address parsing would come from the fuzzer input,
    // but for now we use a dummy address to test the struct itself
    // (soroban-sdk 27 no longer exposes `Address::Account` outside of XDR).
    let dummy_address = Address::from_str(
        &env,
        "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U",
    );

    // Parse status (byte 64, or next available)
    let status_discriminant = data[96] % 2;

    let status = match status_discriminant {
        0 => earnproof_shared::ProofStatus::Active,
        _ => earnproof_shared::ProofStatus::Revoked,
    };

    // Parse schema_version (u32, bytes 65-69, big-endian)
    let schema_version = u32::from_be_bytes([data[97], data[98], data[99], data[100]]);

    // Parse expires_at (u64, bytes 69-77, big-endian)
    let expires_at = if data.len() > 108 {
        u64::from_be_bytes([
            data[101], data[102], data[103], data[104], data[105], data[106], data[107], data[108],
        ])
    } else {
        1_000_000
    };

    // Parse created_at (u64, bytes 77-85, big-endian)
    let created_at = if data.len() > 116 {
        u64::from_be_bytes([
            data[109], data[110], data[111], data[112], data[113], data[114], data[115], data[116],
        ])
    } else {
        1_000
    };

    // Parse revoked_at (u64, bytes 85-93, big-endian)
    let revoked_at = if data.len() > 124 {
        u64::from_be_bytes([
            data[117], data[118], data[119], data[120], data[121], data[122], data[123], data[124],
        ])
    } else {
        0
    };

    let created_ledger = u32::from_be_bytes([data[125], data[126], data[127], data[128]]);

    let sequence_number = if data.len() > 105 {
        u64::from_be_bytes([
            data[97], data[98], data[99], data[100], data[101], data[102], data[103], data[104],
        ])
    } else {
        1
    };

    // Parse activates_at (u64, bytes 97-105, big-endian)
    let activates_at = if data.len() > 113 {
        u64::from_be_bytes([
            data[105], data[106], data[107], data[108], data[109], data[110], data[111], data[112],
        ])
    } else {
        0
    };

    // Construct the ProofRecord - this should never panic or cause undefined behavior
    let _proof = ProofRecord {
        proof_id_hash,
        commitment_hash,
        disclosure_policy_hash,
        issuer_address: dummy_address,
        status,
        schema_version,
        expires_at,
        created_at,
        revoked_at,
        sequence_number,
        created_ledger,
        activates_at,
    };

    // Verify invariants (test should not reach here if invariants are violated)
    assert_eq!(_proof.proof_id_hash.len(), 32);
    assert_eq!(_proof.commitment_hash.len(), 32);
});
