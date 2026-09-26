import os

def update_contract(path, contract_name):
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    # 1. Add PendingAdmin to DataKey
    if "PendingAdmin," not in content:
        content = content.replace("Decommissioned,", "Decommissioned,\n    PendingAdmin,")
        content = content.replace("SchemaVersion(u32),", "SchemaVersion(u32),\n    PendingAdmin,") # fallback just in case
        content = content.replace("UpgradeApproval,", "UpgradeApproval,\n    PendingAdmin,") # issuer-registry

    # 2. Add new events
    events = """
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
"""
    if "AdminTransferNominated" not in content:
        if "pub struct AdminChanged" in content:
            content = content.replace("pub struct AdminChanged {\n    pub new_admin: Address,\n}", "pub struct AdminChanged {\n    pub new_admin: Address,\n}\n" + events)
        else:
            # find another suitable place, like before `// ── upgrade events`
            content = content.replace("// ── upgrade events ────────────────────────────────────────────────────────────", events + "\n// ── upgrade events ────────────────────────────────────────────────────────────")

    # 3. Add or replace methods
    methods = """
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
"""
    if "protocol-config" in contract_name:
        methods = methods.replace("{BUMP_VERSION}", "Self::bump_config_version(env.clone());")
        methods = methods.replace("Self::require_valid_admin", "Self::require_valid_principal")
        methods = methods.replace("{MAP_ERR}", "")
        
        import re
        content = re.sub(
            r"    pub fn set_admin\(env: Env, new_admin: Address\) -> Result<\(\), ContractError> \{[\s\S]*?    \}",
            methods.strip(),
            content
        )
    elif "issuer-registry" in contract_name:
        methods = methods.replace("{BUMP_VERSION}", "")
        methods = methods.replace("Self::require_valid_admin", "Self::require_valid_admin")
        methods = methods.replace("{MAP_ERR}", ".map_err(|_| ContractError::InvalidState)")
        if "nominate_admin" not in content:
            content = content.replace("    pub fn get_admin(env: Env) -> Result<Address, ContractError> {", methods + "\n    pub fn get_admin(env: Env) -> Result<Address, ContractError> {")
    elif "proof-registry" in contract_name:
        methods = methods.replace("{BUMP_VERSION}", "")
        methods = methods.replace("Self::require_valid_admin", "Self::require_valid_principal")
        methods = methods.replace("{MAP_ERR}", ".map_err(|_| ContractError::InvalidState)")
        if "nominate_admin" not in content:
            content = content.replace("    pub fn get_admin(env: Env) -> Result<Address, ContractError> {", methods + "\n    pub fn get_admin(env: Env) -> Result<Address, ContractError> {")

    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

update_contract('contracts/protocol-config/src/lib.rs', 'protocol-config')
update_contract('contracts/issuer-registry/src/lib.rs', 'issuer-registry')
update_contract('contracts/proof-registry/src/lib.rs', 'proof-registry')
