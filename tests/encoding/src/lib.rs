//! Golden backend encoding vectors consumed without a JSON runtime dependency.

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod contract_compatibility;

#[cfg(test)]
mod tests {
    use earnproof_shared::{LEGACY_COMMITMENT_ALGORITHM, SHA256_COMMITMENT_ALGORITHM_V1};
    use sha2::{Digest, Sha256};

    #[test]
    fn sha256_vectors_match_published_hex() {
        for line in include_str!("../../fixtures/encoding/vectors.tsv").lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields[1] == "sha256" || fields[1] == "policy-json-sha256" {
                let digest = Sha256::digest(fields[2].as_bytes());
                assert_eq!(format!("{digest:x}"), fields[3], "{}", fields[0]);
            }
            assert_eq!(fields[3], fields[4], "{}", fields[0]);
        }
    }

    #[test]
    fn malformed_vectors_are_rejected_by_boundary_rules() {
        for line in include_str!("../../fixtures/encoding/invalid.tsv").lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            let valid = !fields[2].is_empty()
                && fields[3].len() == 64
                && fields[3]
                    .chars()
                    .all(|character| character.is_ascii_hexdigit());
            assert!(!valid, "{} must be rejected", fields[0]);
        }
    }

    #[test]
    fn commitment_algorithm_known_vectors_are_stable() {
        let payload = b"proof:example:1";
        let legacy = Sha256::digest(payload);
        assert_eq!(LEGACY_COMMITMENT_ALGORITHM, 0);
        assert_eq!(format!("{legacy:x}"), "c5aecb1a93a48d868c6708d746a71d7eb57f0cfd7a18f0659f97d34fc63efa19");

        let mut versioned_hasher = Sha256::new();
        versioned_hasher.update(b"earnproof:proof-commitment:v1\0");
        versioned_hasher.update(payload);
        let versioned = versioned_hasher.finalize();
        assert_eq!(SHA256_COMMITMENT_ALGORITHM_V1, 1);
        assert_eq!(format!("{versioned:x}"), "401f9532c86efbb3b12e265287875c82792657c04834d25b7f6736649ae535f4");
    }
}
