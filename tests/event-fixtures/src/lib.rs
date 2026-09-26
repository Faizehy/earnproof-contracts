#![allow(dead_code)]

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, PartialEq)]
struct PayloadField {
    #[serde(rename = "type")]
    field_type: String,
    description: String,
    #[serde(default)]
    example: serde_json::Value,
}

#[derive(Debug, Deserialize, PartialEq)]
struct EventFixture {
    event: String,
    contract: String,
    contract_version: String,
    schema_version: u32,
    topics: Vec<String>,
    payload: HashMap<String, PayloadField>,
    emitted_by: String,
    description: String,
    compatibility: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct NoEventsContract {
    contract: String,
    contract_version: String,
    schema_version: u32,
    events: Vec<serde_json::Value>,
    description: String,
}

fn workspace_root() -> &'static Path {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("must have workspace root")
}

fn fixtures_dir() -> String {
    let root = workspace_root();
    root.join("tests/fixtures/events")
        .to_str()
        .expect("path must be valid UTF-8")
        .to_string()
}

fn load_fixture(path: &str) -> EventFixture {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("failed to parse {path}: {e}"))
}

fn load_no_events_fixture(path: &str) -> NoEventsContract {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("failed to parse {path}: {e}"))
}

fn read_contract_version(contract: &str) -> String {
    let root = workspace_root();
    let cargo_toml = root
        .join(format!("contracts/{contract}/Cargo.toml"))
        .to_str()
        .expect("path must be UTF-8")
        .to_string();
    let content =
        fs::read_to_string(&cargo_toml).unwrap_or_else(|e| panic!("read {cargo_toml}: {e}"));
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(v) = trimmed.strip_prefix("version = ") {
            return v.trim_matches('"').to_string();
        }
    }
    panic!("version not found in {cargo_toml}");
}

// ---------------------------------------------------------------------------
// Protocol-config: 6 event fixtures
// ---------------------------------------------------------------------------

const PROTOCOL_CONFIG_EVENTS: &[&str] = &[
    "initialized",
    "admin-transfer-nominated",
    "admin-transfer-accepted",
    "admin-transfer-cancelled",
    "paused",
    "unpaused",
    "schema-approved",
    "schema-deprecated",
];

#[test]
fn protocol_config_fixtures_match_contract_version() {
    let version = read_contract_version("protocol-config");
    let dir = fixtures_dir();
    let dir = format!("{dir}/protocol-config/v1");
    for name in PROTOCOL_CONFIG_EVENTS {
        let path = format!("{dir}/{name}.json");
        let fixture = load_fixture(&path);
        assert_eq!(
            fixture.contract_version, version,
            "{name}.json contract_version mismatch"
        );
        assert_eq!(fixture.contract, "protocol-config");
        assert_eq!(fixture.schema_version, 1);
    }
}

#[test]
fn protocol_config_fixtures_have_required_fields() {
    let base = fixtures_dir();
    let dir = format!("{base}/protocol-config/v1");
    for name in PROTOCOL_CONFIG_EVENTS {
        let path = format!("{dir}/{name}.json");
        let fixture = load_fixture(&path);
        assert!(
            !fixture.topics.is_empty(),
            "{name}.json must have at least one topic"
        );
        assert!(
            !fixture.emitted_by.is_empty(),
            "{name}.json must specify emitted_by"
        );
        assert!(
            !fixture.description.is_empty(),
            "{name}.json must have a description"
        );
        assert!(
            fixture.compatibility == "stable"
                || fixture.compatibility == "additive"
                || fixture.compatibility == "breaking",
            "{name}.json has invalid compatibility: {}",
            fixture.compatibility
        );
    }
}

#[test]
fn protocol_config_event_names_match_topics() {
    let base = fixtures_dir();
    let dir = format!("{base}/protocol-config/v1");
    for name in PROTOCOL_CONFIG_EVENTS {
        let path = format!("{dir}/{name}.json");
        let fixture = load_fixture(&path);
        assert_eq!(
            fixture.topics[0], fixture.event,
            "{name}.json: first topic must equal event name"
        );
    }
}

#[test]
fn protocol_config_no_private_data_in_fixtures() {
    let forbidden_keywords = ["salary", "income", "email", "phone", "ssn", "private"];
    let base = fixtures_dir();
    let dir = format!("{base}/protocol-config/v1");
    for name in PROTOCOL_CONFIG_EVENTS {
        let path = format!("{dir}/{name}.json");
        let content = fs::read_to_string(&path).unwrap();
        let lower = content.to_lowercase();
        for keyword in &forbidden_keywords {
            assert!(
                !lower.contains(keyword),
                "{name}.json contains forbidden keyword '{keyword}'"
            );
        }
    }
}

#[test]
fn protocol_config_all_event_fixtures_exist() {
    let base = fixtures_dir();
    let dir = format!("{base}/protocol-config/v1");
    for name in PROTOCOL_CONFIG_EVENTS {
        let path = format!("{dir}/{name}.json");
        assert!(Path::new(&path).exists(), "missing fixture: {path}");
    }
}

// ---------------------------------------------------------------------------
// Issuer-registry: no events currently
// ---------------------------------------------------------------------------

#[test]
fn issuer_registry_no_events_fixture_exists() {
    let base = fixtures_dir();
    let path = format!("{base}/issuer-registry/v1/events.json");
    assert!(Path::new(&path).exists(), "missing fixture: {path}");
}

#[test]
fn issuer_registry_no_events_fixture_valid() {
    let base = fixtures_dir();
    let path = format!("{base}/issuer-registry/v1/events.json");
    let fixture = load_no_events_fixture(&path);
    assert_eq!(fixture.contract, "issuer-registry");
    assert!(fixture.events.is_empty());
}

#[test]
fn issuer_registry_fixture_matches_contract_version() {
    let version = read_contract_version("issuer-registry");
    let base = fixtures_dir();
    let path = format!("{base}/issuer-registry/v1/events.json");
    let fixture = load_no_events_fixture(&path);
    assert_eq!(fixture.contract_version, version);
}

// ---------------------------------------------------------------------------
// Proof-registry: no events currently
// ---------------------------------------------------------------------------

#[test]
fn proof_registry_no_events_fixture_exists() {
    let base = fixtures_dir();
    let path = format!("{base}/proof-registry/v1/events.json");
    assert!(Path::new(&path).exists(), "missing fixture: {path}");
}

#[test]
fn proof_registry_no_events_fixture_valid() {
    let base = fixtures_dir();
    let path = format!("{base}/proof-registry/v1/events.json");
    let fixture = load_no_events_fixture(&path);
    assert_eq!(fixture.contract, "proof-registry");
    assert!(fixture.events.is_empty());
}

#[test]
fn proof_registry_fixture_matches_contract_version() {
    let version = read_contract_version("proof-registry");
    let base = fixtures_dir();
    let path = format!("{base}/proof-registry/v1/events.json");
    let fixture = load_no_events_fixture(&path);
    assert_eq!(fixture.contract_version, version);
}

// ---------------------------------------------------------------------------
// Schema version drift detection
// ---------------------------------------------------------------------------

#[test]
fn schema_version_is_consistent_across_fixtures() {
    let base = fixtures_dir();
    let dir = format!("{base}/protocol-config/v1");
    for name in PROTOCOL_CONFIG_EVENTS {
        let path = format!("{dir}/{name}.json");
        let fixture = load_fixture(&path);
        assert_eq!(
            fixture.schema_version, 1,
            "{name}.json schema_version must be 1 for v1 directory"
        );
    }
}

#[test]
fn fixture_files_are_parseable_json() {
    let contracts = ["protocol-config", "issuer-registry", "proof-registry"];
    let base = fixtures_dir();
    for contract in contracts {
        let v1_dir = format!("{base}/{contract}/v1");
        assert!(
            Path::new(&v1_dir).exists(),
            "missing version directory: {v1_dir}"
        );
        let entries = fs::read_dir(&v1_dir).unwrap_or_else(|e| panic!("read {v1_dir}: {e}"));
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let content = fs::read_to_string(&path).unwrap();
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
                assert!(parsed.is_ok(), "invalid JSON in {}", path.display());
            }
        }
    }
}
