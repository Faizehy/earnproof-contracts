const fs = require('fs');

function updateContract(path, contractName) {
    let content = fs.readFileSync(path, 'utf8');

    // 1. Add PendingAdmin to DataKey
    if (!content.includes("PendingAdmin,")) {
        content = content.replace("Decommissioned,", "Decommissioned,\n    PendingAdmin,");
        content = content.replace("SchemaVersion(u32),", "SchemaVersion(u32),\n    PendingAdmin,"); 
        content = content.replace("UpgradeApproval,", "UpgradeApproval,\n    PendingAdmin,"); 
    }

    // 2. Add new events
    const events = `
#[contractevent]
pub struct AdminTransferNominated {
    pub pending_admin: Address,
    pub nominated_by: Address,
}

#[contractevent]
pub struct AdminTransferAccepted {
    pub new_admin: Address,
}

#[contractevent]
pub struct AdminTransferCancelled {
    pub pending_admin: Address,
    pub cancelled_by: Address,
}
`;
    if (!content.includes("AdminTransferNominated")) {
        if (content.includes("pub struct AdminChanged")) {
            content = content.replace("pub struct AdminChanged {\n    pub new_admin: Address,\n}", "pub struct AdminChanged {\n    pub new_admin: Address,\n}\n" + events);
        } else {
            content = content.replace("// ── upgrade events ────────────────────────────────────────────────────────────", events + "\n// ── upgrade events ────────────────────────────────────────────────────────────");
        }
    }

    // 3. Add or replace methods
    let methods = `
    pub fn nominate_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env){MAP_ERR}?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_valid_admin(&new_admin)?;
        Self::require_auth(&admin);
        
        env.storage().instance().set(&DataKey::PendingAdmin, &new_admin);
        AdminTransferNominated {
            pending_admin: new_admin.clone(),
            nominated_by: admin,
        }
        .publish(&env);
        Ok(())
    }

    pub fn accept_admin(env: Env) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env){MAP_ERR}?;
        let pending_admin: Address = env.storage().instance().get(&DataKey::PendingAdmin).ok_or(ContractError::NotFound)?;
        Self::require_auth(&pending_admin);
        
        env.storage().instance().set(&DataKey::Admin, &pending_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        
        {BUMP_VERSION}
        
        AdminTransferAccepted { new_admin: pending_admin }.publish(&env);
        Ok(())
    }

    pub fn cancel_admin_transfer(env: Env) -> Result<(), ContractError> {
        Self::ensure_not_decommissioned(&env){MAP_ERR}?;
        let admin = Self::get_admin(env.clone())?;
        Self::require_auth(&admin);
        
        let pending_admin: Address = env.storage().instance().get(&DataKey::PendingAdmin).ok_or(ContractError::NotFound)?;
        env.storage().instance().remove(&DataKey::PendingAdmin);
        
        AdminTransferCancelled {
            pending_admin,
            cancelled_by: admin,
        }
        .publish(&env);
        Ok(())
    }
`;

    if (contractName === 'protocol-config') {
        methods = methods.replace(/{BUMP_VERSION}/g, "Self::bump_config_version(env.clone());");
        methods = methods.replace(/Self::require_valid_admin/g, "Self::require_valid_principal");
        methods = methods.replace(/{MAP_ERR}/g, "");
        
        // Use regex to replace set_admin
        content = content.replace(/    pub fn set_admin\(env: Env, new_admin: Address\) -> Result<\(\), ContractError> \{[\s\S]*?    \}/, methods.trim());
    } else if (contractName === 'issuer-registry') {
        methods = methods.replace(/{BUMP_VERSION}/g, "");
        methods = methods.replace(/Self::require_valid_admin/g, "Self::require_valid_admin");
        methods = methods.replace(/{MAP_ERR}/g, ".map_err(|_| ContractError::InvalidState)");
        if (!content.includes("nominate_admin")) {
            content = content.replace("    pub fn get_admin(env: Env) -> Result<Address, ContractError> {", methods + "\n    pub fn get_admin(env: Env) -> Result<Address, ContractError> {");
        }
    } else if (contractName === 'proof-registry') {
        methods = methods.replace(/{BUMP_VERSION}/g, "");
        methods = methods.replace(/Self::require_valid_admin/g, "Self::require_valid_principal");
        methods = methods.replace(/{MAP_ERR}/g, ".map_err(|_| ContractError::InvalidState)");
        if (!content.includes("nominate_admin")) {
            content = content.replace("    pub fn get_admin(env: Env) -> Result<Address, ContractError> {", methods + "\n    pub fn get_admin(env: Env) -> Result<Address, ContractError> {");
        }
    }

    fs.writeFileSync(path, content, 'utf8');
}

updateContract('contracts/protocol-config/src/lib.rs', 'protocol-config');
updateContract('contracts/issuer-registry/src/lib.rs', 'issuer-registry');
updateContract('contracts/proof-registry/src/lib.rs', 'proof-registry');
