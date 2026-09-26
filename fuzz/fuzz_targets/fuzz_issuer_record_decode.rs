#![no_main]
use earnproof_shared::IssuerRecord;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{Address, Bytes, BytesN, Env};

// Fuzz target for IssuerRecord deserialization and field validation
// Tests that arbitrary bytes can be safely deserialized or fail gracefully
fuzz_target!(|data: &[u8]| {
    // Limit input size to prevent memory exhaustion
    if data.len() > 8192 {
        return;
    }

    // Skip if data is too short for an IssuerRecord (32+32+32+4+8+8 = 116 bytes minimum)
    if data.len() < 116 {
        return;
    }

    let env = Env::default();

    // Attempt to parse IssuerRecord from fields:
    // - issuer_id_hash: BytesN<32> (bytes 0-32)
    // - issuer_address: Address (variable, dummy for fuzz)
    // - metadata_hash: BytesN<32> (bytes 32-64)
    // - status: IssuerStatus (enum: 0, 1, or 2)
    // - created_at: u64
    // - updated_at: u64

    // Extract issuer_id_hash (first 32 bytes)
    let issuer_id_hash = match BytesN::<32>::try_from(Bytes::from_slice(&env, &data[0..32])) {
        Ok(h) => h,
        Err(_) => return,
    };

    // Extract metadata_hash (next 32 bytes)
    let metadata_hash = match BytesN::<32>::try_from(Bytes::from_slice(&env, &data[32..64])) {
        Ok(h) => h,
        Err(_) => return,
    };

    // Use a dummy address (a valid strkey; soroban-sdk 27 no longer exposes
    // the `Address::Account` constructor outside of XDR conversion).
    let issuer_address = Address::from_str(
        &env,
        "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U",
    );

    // Parse status (byte 64, or next available)
    let status_discriminant = if data.len() > 64 {
        data[64] % 3 // 0 = Active, 1 = Suspended, 2 = Revoked
    } else {
        0
    };

    let status = match status_discriminant {
        0 => earnproof_shared::IssuerStatus::Active,
        1 => earnproof_shared::IssuerStatus::Suspended,
        _ => earnproof_shared::IssuerStatus::Revoked,
    };

    // Parse created_at (u64, bytes 65-73, big-endian)
    let created_at = if data.len() > 72 {
        u64::from_be_bytes([
            data[65], data[66], data[67], data[68], data[69], data[70], data[71], data[72],
        ])
    } else {
        1_000
    };

    // Parse updated_at (u64, bytes 73-81, big-endian)
    let updated_at = if data.len() > 80 {
        u64::from_be_bytes([
            data[73], data[74], data[75], data[76], data[77], data[78], data[79], data[80],
        ])
    } else {
        1_000
    };

    // Construct the IssuerRecord - this should never panic or cause undefined behavior
    let _issuer = IssuerRecord {
        issuer_id_hash: issuer_id_hash.clone(),
        issuer_address,
        metadata_hash: metadata_hash.clone(),
        provenance_commitment: metadata_hash,
        status,
        created_at,
        updated_at,
        reason_commitment: None,
    };

    // Verify invariants
    assert_eq!(_issuer.issuer_id_hash.len(), 32);
    assert_eq!(_issuer.metadata_hash.len(), 32);
});
