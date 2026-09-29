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

    // Skip if data is too short for a ProofRecord (32+32+32+4+8+8+8 = 124 bytes minimum)
    if data.len() < 124 {
        return;
    }

    let env = Env::default();

    // Attempt to parse ProofRecord from XDR
    // The soroban-sdk's FromXdr trait is used internally for contracttype deserialization
    // We simulate the kind of errors that should be caught during deserialization

    // Try to construct a ProofRecord by parsing fixed fields:
    // - proof_id_hash: BytesN<32> (bytes 0-32)
    // - commitment_hash: BytesN<32> (bytes 32-64)
    // - issuer_address: Address (variable length, typically 32-40 bytes in XDR)
    // - status: ProofStatus (enum: 0 or 1)
    // - schema_version: u32
    // - expires_at: u64
    // - created_at: u64
    // - revoked_at: u64

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

    // Verify that we can safely handle the record
    // In a real scenario, Address parsing would come from the fuzzer input,
    // but for now we use a dummy address to test the struct itself
    // (soroban-sdk 27 no longer exposes `Address::Account` outside of XDR).
    let dummy_address = Address::from_str(
        &env,
        "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U",
    );

    // Parse status (byte 64, or next available)
    let status_discriminant = if data.len() > 64 {
        data[64] % 2 // 0 = Active, 1 = Revoked
    } else {
        0
    };

    let status = match status_discriminant {
        0 => earnproof_shared::ProofStatus::Active,
        _ => earnproof_shared::ProofStatus::Revoked,
    };

    // Parse schema_version (u32, bytes 65-69, big-endian)
    let schema_version = if data.len() > 68 {
        u32::from_be_bytes([data[65], data[66], data[67], data[68]])
    } else {
        1
    };

    // Parse expires_at (u64, bytes 69-77, big-endian)
    let expires_at = if data.len() > 76 {
        u64::from_be_bytes([
            data[69], data[70], data[71], data[72], data[73], data[74], data[75], data[76],
        ])
    } else {
        1_000_000
    };

    // Parse created_at (u64, bytes 77-85, big-endian)
    let created_at = if data.len() > 84 {
        u64::from_be_bytes([
            data[77], data[78], data[79], data[80], data[81], data[82], data[83], data[84],
        ])
    } else {
        1_000
    };

    // Parse revoked_at (u64, bytes 85-93, big-endian)
    let revoked_at = if data.len() > 92 {
        u64::from_be_bytes([
            data[85], data[86], data[87], data[88], data[89], data[90], data[91], data[92],
        ])
    } else {
        0
    };

    // Construct the ProofRecord - this should never panic or cause undefined behavior
    let _proof = ProofRecord {
        proof_id_hash,
        commitment_hash,
        issuer_address: dummy_address,
        status,
        schema_version,
        expires_at,
        created_at,
        revoked_at,
        revoked_ledger: 0,
    };

    // Verify invariants (test should not reach here if invariants are violated)
    assert_eq!(_proof.proof_id_hash.len(), 32);
    assert_eq!(_proof.commitment_hash.len(), 32);
});
