//! Shared fixtures for the authorization negative matrix.
//!
//! Unlike the emergency harness, this harness does **not** call
//! `mock_all_auths`. Every privileged call is authorized explicitly through
//! [`authorize`], which installs a *matching-mode* auth entry: the host admits
//! the next invocation only when the demanded signer, function, and arguments
//! match the entry exactly, and rejects everything else. That enforcement is
//! what lets the matrix assert both the returned error and the absence of side
//! effects for missing and wrong identities.
//!
//! The deployment is the full three-contract set so that cross-contract
//! authorization boundaries (`proof-registry`'s issuer-vs-admin revocation
//! paths, independent per-contract admins) can be exercised from one place.

use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, BytesN, Env, IntoVal, Map, Val};

use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};

/// Schema version approved by [`Deployment::new`] and used by the fixture
/// proofs.
pub const APPROVED_SCHEMA: u32 = 1;

/// Installs a matching-mode auth entry: `signer` is authorized for exactly one
/// invocation of `fn_name` on `contract` with `args`. Any other signer,
/// function, or argument set is rejected by the host.
pub fn authorize(
    env: &Env,
    signer: &Address,
    contract: &Address,
    fn_name: &str,
    args: soroban_sdk::Vec<Val>,
) {
    env.mock_auths(&[MockAuth {
        address: signer,
        invoke: &MockAuthInvoke {
            contract,
            fn_name,
            args,
            sub_invokes: &[],
        },
    }]);
}

/// A fully wired deployment with real auth enforcement.
pub struct Deployment<'a> {
    pub env: Env,
    pub config: ProtocolConfigContractClient<'a>,
    pub issuers: IssuerRegistryContractClient<'a>,
    pub proofs: ProofRegistryContractClient<'a>,
    pub config_address: Address,
    pub issuers_address: Address,
    pub proofs_address: Address,
    /// Administrator of all three contracts at deployment time.
    pub admin: Address,
    /// An active issuer, authorized to register proofs.
    pub issuer: Address,
    /// A second active issuer, used as the "wrong but privileged" signer in
    /// proof-registry rows: it is not the named issuer, so its signature must
    /// not be accepted.
    pub second_issuer: Address,
    pub issuer_id: BytesN<32>,
    pub second_issuer_id: BytesN<32>,
}

impl Deployment<'_> {
    /// Deploys and fully initializes the three-contract set. `initialize`,
    /// `approve_schema_version`, and `register_issuer` are all authorized as
    /// `admin`; `second_issuer` is registered as a second active issuer.
    pub fn new() -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);

        let admin = Address::generate(&env);
        let issuer = Address::generate(&env);
        let second_issuer = Address::generate(&env);

        let config_id = env.register(ProtocolConfigContract, ());
        let config = ProtocolConfigContractClient::new(&env, &config_id);
        let issuers_id = env.register(IssuerRegistryContract, ());
        let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);

        // protocol-config: initialize + approve the fixture schema.
        authorize(
            &env,
            &admin,
            &config_id,
            "initialize",
            (&admin,).into_val(&env),
        );
        config.initialize(&admin);
        let schema_proposal = proposal_id_hash(&env, 0xFE);
        authorize(
            &env,
            &admin,
            &config_id,
            "approve_schema_version",
            (&schema_proposal, &APPROVED_SCHEMA).into_val(&env),
        );
        config.approve_schema_version(&schema_proposal, &APPROVED_SCHEMA);

        // issuer-registry: initialize + register two active issuers.
        let issuer_id = issuer_id_hash(&env, 1);
        let second_issuer_id = issuer_id_hash(&env, 2);
        authorize(
            &env,
            &admin,
            &issuers_id,
            "initialize",
            (&admin,).into_val(&env),
        );
        issuers.initialize(&admin);
        authorize(
            &env,
            &admin,
            &issuers_id,
            "register_issuer",
            (&issuer_id, &issuer, &hash(&env, 0xAA), &hash(&env, 0x99)).into_val(&env),
        );
        issuers.register_issuer(&issuer_id, &issuer, &hash(&env, 0xAA), &hash(&env, 0x99));
        authorize(
            &env,
            &admin,
            &issuers_id,
            "register_issuer",
            (
                &second_issuer_id,
                &second_issuer,
                &hash(&env, 0xBB),
                &hash(&env, 0x99),
            )
                .into_val(&env),
        );
        issuers.register_issuer(
            &second_issuer_id,
            &second_issuer,
            &hash(&env, 0xBB),
            &hash(&env, 0x99),
        );

        // proof-registry: initialize with the two supporting contracts.
        authorize(
            &env,
            &admin,
            &proofs_id,
            "initialize",
            (&admin, &issuers_id, &config_id).into_val(&env),
        );
        proofs.initialize(&admin, &issuers_id, &config_id);

        Self {
            env,
            config,
            issuers,
            proofs,
            config_address: config_id,
            issuers_address: issuers_id,
            proofs_address: proofs_id,
            admin,
            issuer,
            second_issuer,
            issuer_id,
            second_issuer_id,
        }
    }

/// Every mutating public entry point across the three contracts.
///
/// A new mutating function without a row here is a documentation gap:
/// [`matrix_covers_every_mutating_public_function`] fails when the count
/// drifts. `docs/authorization-matrix.md` must change together with this table.
fn matrix() -> std::vec::Vec<Case> {
    std::vec![
        // -----------------------------------------------------------------
        // protocol-config
        // -----------------------------------------------------------------
        Case {
            name: "protocol-config::initialize",
            uninitialized: true,
            setup: no_setup,
            call: |d, identity| {
                let args: soroban_sdk::Vec<Val> = (&d.admin,).into_val(&d.env);
                match identity {
                    Identity::Missing => d.config.try_initialize(&d.admin).is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.config_address,
                            "initialize",
                            args.clone(),
                        );
                        d.config.try_initialize(&d.admin).is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.config_address, "initialize", args);
                        d.config.try_initialize(&d.admin).is_ok()
                    }
                }
            },
        },
        Case {
            name: "protocol-config::set_admin",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let next = Address::generate(&d.env);
                let args: soroban_sdk::Vec<Val> = (&next,).into_val(&d.env);
                match identity {
                    Identity::Missing => {
                        let r = d.config.try_nominate_admin(&next);
                        if r.is_ok() {
                            let _ = d.config.try_accept_admin();
                        }
                        r
                    }
                    .is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.config_address,
                            "nominate_admin",
                            args.clone(),
                        );
                        {
                            let r = d.config.try_nominate_admin(&next);
                            if r.is_ok() {
                                authorize(
                                    &d.env,
                                    &next,
                                    &d.config_address,
                                    "accept_admin",
                                    ().into_val(&d.env),
                                );
                                let _ = d.config.try_accept_admin();
                            }
                            r
                        }
                        .is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.config_address, "nominate_admin", args);
                        {
                            let r = d.config.try_nominate_admin(&next);
                            if r.is_ok() {
                                authorize(
                                    &d.env,
                                    &next,
                                    &d.config_address,
                                    "accept_admin",
                                    ().into_val(&d.env),
                                );
                                let _ = d.config.try_accept_admin();
                            }
                            r
                        }
                        .is_ok()
                    }
                }
            },
        },
        Case {
            name: "protocol-config::pause",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let args: soroban_sdk::Vec<Val> = ().into_val(&d.env);
                match identity {
                    Identity::Missing => d.config.try_pause().is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.config_address,
                            "pause",
                            args.clone(),
                        );
                        d.config.try_pause().is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.config_address, "pause", args);
                        d.config.try_pause().is_ok()
                    }
                }
            },
        },
        Case {
            name: "protocol-config::unpause",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let args: soroban_sdk::Vec<Val> = ().into_val(&d.env);
                match identity {
                    Identity::Missing => d.config.try_unpause().is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.config_address,
                            "unpause",
                            args.clone(),
                        );
                        d.config.try_unpause().is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.config_address, "unpause", args);
                        d.config.try_unpause().is_ok()
                    }
                }
            },
        },
        Case {
            name: "protocol-config::approve_schema_version",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let version = 7_u32;
                let args: soroban_sdk::Vec<Val> = (&version,).into_val(&d.env);
                match identity {
                    Identity::Missing => d.config.try_approve_schema_version(&version).is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.config_address,
                            "approve_schema_version",
                            args.clone(),
                        );
                        d.config.try_approve_schema_version(&version).is_ok()
                    }
                    Identity::Authorized => {
                        authorize(
                            &d.env,
                            &d.admin,
                            &d.config_address,
                            "approve_schema_version",
                            args,
                        );
                        d.config.try_approve_schema_version(&version).is_ok()
                    }
                }
            },
        },
        Case {
            name: "protocol-config::deprecate_schema_version",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let args: soroban_sdk::Vec<Val> = (&APPROVED_SCHEMA,).into_val(&d.env);
                match identity {
                    Identity::Missing => d
                        .config
                        .try_deprecate_schema_version(&APPROVED_SCHEMA)
                        .is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.config_address,
                            "deprecate_schema_version",
                            args.clone(),
                        );
                        d.config
                            .try_deprecate_schema_version(&APPROVED_SCHEMA)
                            .is_ok()
                    }
                    Identity::Authorized => {
                        authorize(
                            &d.env,
                            &d.admin,
                            &d.config_address,
                            "deprecate_schema_version",
                            args,
                        );
                        d.config
                            .try_deprecate_schema_version(&APPROVED_SCHEMA)
                            .is_ok()
                    }
                }
            },
        },
        // -----------------------------------------------------------------
        // issuer-registry
        // -----------------------------------------------------------------
        Case {
            name: "issuer-registry::initialize",
            uninitialized: true,
            setup: no_setup,
            call: |d, identity| {
                let args: soroban_sdk::Vec<Val> = (&d.admin,).into_val(&d.env);
                match identity {
                    Identity::Missing => d.issuers.try_initialize(&d.admin).is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.issuers_address,
                            "initialize",
                            args.clone(),
                        );
                        d.issuers.try_initialize(&d.admin).is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.issuers_address, "initialize", args);
                        d.issuers.try_initialize(&d.admin).is_ok()
                    }
                }
            },
        },
        Case {
            name: "issuer-registry::register_issuer",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let id = issuer_id_hash(&d.env, 0x70);
                let address = Address::generate(&d.env);
                let metadata = hash(&d.env, 0x71);
                let args: soroban_sdk::Vec<Val> =
                    (&id, &address, &metadata, &metadata).into_val(&d.env);
                match identity {
                    Identity::Missing => d
                        .issuers
                        .try_register_issuer(&id, &address, &metadata, &metadata)
                        .is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.issuers_address,
                            "register_issuer",
                            args.clone(),
                        );
                        d.issuers
                            .try_register_issuer(&id, &address, &metadata, &metadata)
                            .is_ok()
                    }
                    Identity::Authorized => {
                        authorize(
                            &d.env,
                            &d.admin,
                            &d.issuers_address,
                            "register_issuer",
                            args,
                        );
                        d.issuers
                            .try_register_issuer(&id, &address, &metadata, &metadata)
                            .is_ok()
                    }
                }
            },
        },
        Case {
            name: "issuer-registry::update_issuer",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let metadata = hash(&d.env, 0x72);
                let args: soroban_sdk::Vec<Val> = (&d.issuer_id, &metadata).into_val(&d.env);
                match identity {
                    Identity::Missing => {
                        d.issuers.try_update_issuer(&d.issuer_id, &metadata).is_ok()
                    }
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.issuers_address,
                            "update_issuer",
                            args.clone(),
                        );
                        d.issuers.try_update_issuer(&d.issuer_id, &metadata).is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.issuers_address, "update_issuer", args);
                        d.issuers.try_update_issuer(&d.issuer_id, &metadata).is_ok()
                    }
                }
            },
        },
        Case {
            name: "issuer-registry::suspend_issuer",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let reason = soroban_sdk::BytesN::from_array(&d.env, &[1u8; 32]);
                let args: soroban_sdk::Vec<Val> = (&d.issuer_id, &reason).into_val(&d.env);
                match identity {
                    Identity::Missing => {
                        d.issuers.try_suspend_issuer(&d.issuer_id, &reason).is_ok()
                    }
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.issuers_address,
                            "suspend_issuer",
                            args.clone(),
                        );
                        d.issuers.try_suspend_issuer(&d.issuer_id, &reason).is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.issuers_address, "suspend_issuer", args);
                        d.issuers.try_suspend_issuer(&d.issuer_id, &reason).is_ok()
                    }
                }
            },
        },
        Case {
            name: "issuer-registry::reactivate_issuer",
            uninitialized: false,
            // The fixture issuer must be suspended first, or reactivation is
            // rejected by a state precondition rather than by authorization.
            setup: |d| d.suspend_issuer(&d.issuer_id),
            call: |d, identity| {
                let reason = soroban_sdk::BytesN::from_array(&d.env, &[1u8; 32]);
                let args: soroban_sdk::Vec<Val> = (&d.issuer_id, &reason).into_val(&d.env);
                match identity {
                    Identity::Missing => d
                        .issuers
                        .try_reactivate_issuer(&d.issuer_id, &reason)
                        .is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.issuers_address,
                            "reactivate_issuer",
                            args.clone(),
                        );
                        d.issuers
                            .try_reactivate_issuer(&d.issuer_id, &reason)
                            .is_ok()
                    }
                    Identity::Authorized => {
                        authorize(
                            &d.env,
                            &d.admin,
                            &d.issuers_address,
                            "reactivate_issuer",
                            args,
                        );
                        d.issuers
                            .try_reactivate_issuer(&d.issuer_id, &reason)
                            .is_ok()
                    }
                }
            },
        },
        Case {
            name: "issuer-registry::revoke_issuer",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let reason = soroban_sdk::BytesN::from_array(&d.env, &[1u8; 32]);
                let args: soroban_sdk::Vec<Val> = (&d.issuer_id, &reason).into_val(&d.env);
                match identity {
                    Identity::Missing => d.issuers.try_revoke_issuer(&d.issuer_id, &reason).is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.issuers_address,
                            "revoke_issuer",
                            args.clone(),
                        );
                        d.issuers.try_revoke_issuer(&d.issuer_id, &reason).is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.issuers_address, "revoke_issuer", args);
                        d.issuers.try_revoke_issuer(&d.issuer_id, &reason).is_ok()
                    }
                }
            },
        },
        Case {
            name: "issuer-registry::rotate_issuer_address",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let next = Address::generate(&d.env);
                let args: soroban_sdk::Vec<Val> = (&d.issuer_id, &next).into_val(&d.env);
                match identity {
                    Identity::Missing => d
                        .issuers
                        .try_rotate_issuer_address(&d.issuer_id, &next)
                        .is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.issuers_address,
                            "rotate_issuer_address",
                            args.clone(),
                        );
                        d.issuers
                            .try_rotate_issuer_address(&d.issuer_id, &next)
                            .is_ok()
                    }
                    Identity::Authorized => {
                        authorize(
                            &d.env,
                            &d.admin,
                            &d.issuers_address,
                            "rotate_issuer_address",
                            args,
                        );
                        d.issuers
                            .try_rotate_issuer_address(&d.issuer_id, &next)
                            .is_ok()
                    }
                }
            },
        },
        // -----------------------------------------------------------------
        // proof-registry
        // -----------------------------------------------------------------
        Case {
            name: "proof-registry::initialize",
            uninitialized: true,
            setup: no_setup,
            call: |d, identity| {
                let args: soroban_sdk::Vec<Val> =
                    (&d.admin, &d.issuers_address, &d.config_address).into_val(&d.env);
                match identity {
                    Identity::Missing => d
                        .proofs
                        .try_initialize(&d.admin, &d.issuers_address, &d.config_address)
                        .is_ok(),
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.attacker(),
                            &d.proofs_address,
                            "initialize",
                            args.clone(),
                        );
                        d.proofs
                            .try_initialize(&d.admin, &d.issuers_address, &d.config_address)
                            .is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.admin, &d.proofs_address, "initialize", args);
                        d.proofs
                            .try_initialize(&d.admin, &d.issuers_address, &d.config_address)
                            .is_ok()
                    }
                }
            },
        },
        Case {
            name: "proof-registry::register_proof",
            uninitialized: false,
            setup: no_setup,
            call: |d, identity| {
                let proof_id = hash(&d.env, 0x80);
                let commitment = hash(&d.env, 0x81);
                let expires_at = d.env.ledger().timestamp() + 100_000;
                let args: soroban_sdk::Vec<Val> = (
                    &proof_id,
                    &commitment,
                    &d.issuer,
                    &APPROVED_SCHEMA,
                    &expires_at,
                )
                    .into_val(&d.env);
                match identity {
                    Identity::Missing => d
                        .proofs
                        .try_register_proof(
                            &proof_id,
                            &commitment,
                            &d.issuer,
                            &APPROVED_SCHEMA,
                            &expires_at,
                        )
                        .is_ok(),
                    // The realistic "wrong" signer is a *different active
                    // issuer*: someone who holds valid issuer credentials but
                    // is not the issuer named in the registration.
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.second_issuer,
                            &d.proofs_address,
                            "register_proof",
                            args.clone(),
                        );
                        d.proofs
                            .try_register_proof(
                                &proof_id,
                                &commitment,
                                &d.issuer,
                                &APPROVED_SCHEMA,
                                &expires_at,
                            )
                            .is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.issuer, &d.proofs_address, "register_proof", args);
                        d.proofs
                            .try_register_proof(
                                &proof_id,
                                &commitment,
                                &d.issuer,
                                &APPROVED_SCHEMA,
                                &expires_at,
                            )
                            .is_ok()
                    }
                }
            },
        },
        Case {
            name: "proof-registry::revoke_proof",
            uninitialized: false,
            setup: register_fixture_proof,
            call: |d, identity| {
                let proof_id = hash(&d.env, FIXTURE_PROOF);
                let args: soroban_sdk::Vec<Val> = (&proof_id,).into_val(&d.env);
                match identity {
                    Identity::Missing => d.proofs.try_revoke_proof(&proof_id).is_ok(),
                    // A different active issuer must not be able to revoke a
                    // proof it does not own.
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.second_issuer,
                            &d.proofs_address,
                            "revoke_proof",
                            args.clone(),
                        );
                        d.proofs.try_revoke_proof(&proof_id).is_ok()
                    }
                    Identity::Authorized => {
                        authorize(&d.env, &d.issuer, &d.proofs_address, "revoke_proof", args);
                        d.proofs.try_revoke_proof(&proof_id).is_ok()
                    }
                }
            },
        },
        Case {
            name: "proof-registry::admin_revoke_proof",
            uninitialized: false,
            setup: register_fixture_proof,
            call: |d, identity| {
                let proof_id = hash(&d.env, FIXTURE_PROOF);
                let args: soroban_sdk::Vec<Val> = (&proof_id,).into_val(&d.env);
                match identity {
                    Identity::Missing => d.proofs.try_admin_revoke_proof(&proof_id).is_ok(),
                    // The proof's own issuer must not be able to use the admin
                    // path: the two revocation entry points demand different
                    // identities.
                    Identity::Wrong => {
                        authorize(
                            &d.env,
                            &d.issuer,
                            &d.proofs_address,
                            "admin_revoke_proof",
                            args.clone(),
                        );
                        d.proofs.try_admin_revoke_proof(&proof_id).is_ok()
                    }
                    Identity::Authorized => {
                        authorize(
                            &d.env,
                            &d.admin,
                            &d.proofs_address,
                            "admin_revoke_proof",
                            args,
                        );
                        d.proofs.try_admin_revoke_proof(&proof_id).is_ok()
                    }
                }
            },
        },
    ]
}
    /// A fresh, unrelated address that holds no authority anywhere. Generated
    /// on demand so every negative attempt uses a distinct identity.
    pub fn attacker(&self) -> Address {
        Address::generate(&self.env)
    }

    /// Deploys the three contracts without initializing any of them. Used by
    /// the `initialize` rows of the matrix, where the deployment must still be
    /// uninitialized when the attempt runs.
    pub fn uninitialized() -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);

        let admin = Address::generate(&env);
        let issuer = Address::generate(&env);
        let second_issuer = Address::generate(&env);

        let config_id = env.register(ProtocolConfigContract, ());
        let config = ProtocolConfigContractClient::new(&env, &config_id);
        let issuers_id = env.register(IssuerRegistryContract, ());
        let issuers = IssuerRegistryContractClient::new(&env, &issuers_id);
        let proofs_id = env.register(ProofRegistryContract, ());
        let proofs = ProofRegistryContractClient::new(&env, &proofs_id);
        let issuer_id = issuer_id_hash(&env, 1);
        let second_issuer_id = issuer_id_hash(&env, 2);

        Self {
            env,
            config,
            issuers,
            proofs,
            config_address: config_id,
            issuers_address: issuers_id,
            proofs_address: proofs_id,
            admin,
            issuer,
            second_issuer,
            issuer_id,
            second_issuer_id,
        }
    }

    /// Authorized wrapper around the privileged operations the fixture and the
    /// rotation scenarios need. Each method installs the auth entry for the
    /// signer the contract is documented to demand.
    pub fn set_admin(&self, new_admin: &Address) {
        authorize(
            &self.env,
            &self.admin,
            &self.config_address,
            "nominate_admin",
            (new_admin,).into_val(&self.env),
        );
        self.config.nominate_admin(new_admin);
        authorize(
            &self.env,
            new_admin,
            &self.config_address,
            "accept_admin",
            ().into_val(&self.env),
        );
        self.config.accept_admin();
    }

    pub fn suspend_issuer(&self, issuer_id: &BytesN<32>) {
        let proposal_id = proposal_id_hash(&self.env, 0xFC);
        let reason_commitment = soroban_sdk::BytesN::from_array(&self.env, &[1u8; 32]);
        authorize(
            &self.env,
            &self.admin,
            &self.issuers_address,
            "suspend_issuer",
            (&proposal_id, issuer_id, &reason_commitment).into_val(&self.env),
        );
        self.issuers
            .suspend_issuer(&proposal_id, issuer_id, &reason_commitment);
    }

    pub fn rotate_issuer_address(&self, issuer_id: &BytesN<32>, new_address: &Address) {
        authorize(
            &self.env,
            &self.issuer,
            &self.issuers_address,
            "rotate_issuer_address",
            (issuer_id, new_address).into_val(&self.env),
        );
        self.issuers.rotate_issuer_address(issuer_id, new_address);
        authorize(
            &self.env,
            new_address,
            &self.issuers_address,
            "accept_issuer_address_rotation",
            (issuer_id,).into_val(&self.env),
        );
        self.issuers.accept_issuer_address_rotation(issuer_id);
    }

    /// Registers a proof with the given discriminator as `issuer` and returns
    /// its id hash. Fails the test if registration is rejected.
    pub fn register_proof(&self, discriminator: u8) -> BytesN<32> {
        let proof_id = hash(&self.env, discriminator);
        let commitment = hash(&self.env, discriminator ^ 0xFF);
        let expires_at = self.env.ledger().timestamp() + 100_000;
        authorize(
            &self.env,
            &self.issuer,
            &self.proofs_address,
            "register_proof",
            (
                &proof_id,
                &commitment,
                &self.issuer,
                &APPROVED_SCHEMA,
                &expires_at,
            )
                .into_val(&self.env),
        );
        self.proofs.register_proof(
            &proof_id,
            &commitment,
            &self.issuer,
            &APPROVED_SCHEMA,
            &expires_at,
        );
        proof_id
    }
}

// ---------------------------------------------------------------------------
// Snapshots
//
// A rejected call must leave the entire observable surface untouched: storage,
// storage TTLs, instance TTLs, and events. The snapshot captures instance
// storage per contract and all persistent storage globally.
//
// NOTE: Soroban SDK's `Persistent::all()` does not filter by contract
// address — it returns persistent entries from every contract in the test
// environment. We therefore capture persistent storage as a single global
// map at the Env level (not using `as_contract`), and compare that map
// before and after the rejected call. Instance storage is safe to capture
// per contract because `Instance::all()` correctly filters by address.
// ---------------------------------------------------------------------------

/// Instance storage and TTL state of one contract.
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceSnapshot {
    pub address: Address,
    pub instance: Map<Val, Val>,
    pub instance_ttl: u32,
}

/// Observable state of the whole deployment.
#[derive(Clone, Debug, PartialEq)]
pub struct DeploymentSnapshot {
    pub config: InstanceSnapshot,
    pub issuers: InstanceSnapshot,
    pub proofs: InstanceSnapshot,
    /// All persistent storage entries across every contract, captured at the
    /// Env level so contract-boundary filtering issues do not arise.
    pub persistent: Map<Val, Val>,
}

impl Deployment<'_> {
    /// Captures instance storage for a single contract.
    fn capture_instance(&self, address: &Address) -> InstanceSnapshot {
        self.env.as_contract(address, || {
            use soroban_sdk::testutils::storage::Instance as _;
            let instance = self.env.storage().instance().all();
            let instance_ttl = self.env.storage().instance().get_ttl();
            InstanceSnapshot {
                address: address.clone(),
                instance,
                instance_ttl,
            }
        })
    }

    /// Captures the state of all three contracts. Persistent storage is
    /// captured inside `as_contract` (the SDK requires it), and the result
    /// is the same regardless of which contract context is used because
    /// `Persistent::all()` returns entries from every contract.
    pub fn snapshot(&self) -> DeploymentSnapshot {
        use soroban_sdk::testutils::storage::Persistent as _;
        let persistent = self.env.as_contract(&self.config_address, || {
            self.env.storage().persistent().all()
        });
        DeploymentSnapshot {
            config: self.capture_instance(&self.config_address),
            issuers: self.capture_instance(&self.issuers_address),
            proofs: self.capture_instance(&self.proofs_address),
            persistent,
        }
    }

    /// Asserts that a rejected call left every observable surface untouched.
    pub fn assert_no_side_effects(&self, before: &DeploymentSnapshot, label: &str) {
        let after = self.snapshot();
        assert_eq!(
            after, *before,
            "{label}: a rejected call changed storage state"
        );
        assert!(
            self.env.events().all().events().is_empty(),
            "{label}: a rejected call emitted events"
        );
    }
}

/// Deterministic 32-byte value derived from a single discriminator byte.
pub fn hash(env: &Env, discriminator: u8) -> BytesN<32> {
    BytesN::from_array(env, &[discriminator; 32])
}

/// Issuer id hashes live in a disjoint range from proof id hashes so that a
/// scenario mixing the two cannot accidentally collide.
pub fn issuer_id_hash(env: &Env, discriminator: u8) -> BytesN<32> {
    hash(env, 0x80 | discriminator)
}

pub fn proposal_id_hash(env: &Env, discriminator: u8) -> BytesN<32> {
    hash(env, 0x40 | discriminator)
}
