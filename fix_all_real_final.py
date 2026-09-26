import re

def fix(path):
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    # Undo the bad regex insertions
    content = content.replace("self.env.ledger(, &None).timestamp()", "self.env.ledger().timestamp()")
    content = content.replace("self.expiry(, &None)", "self.expiry()")
    
    if "tests/property/state_machine.rs" in path:
        content = content.replace("&predecessor);", "&None);")

    if "tests/ttl/src/proof_registry_ttl.rs" in path:
        content = content.replace("&predecessor);", "&None);")

    if "tests/authorization/src/harness.rs" in path:
        content = re.sub(
            r'(\.register_proof\([^;]+?,\s*&expires_at)\s*\);',
            r'\1, &None);',
            content
        )

    if "tests/cross-contract/src/harness.rs" in path:
        content = content.replace("                (None)\n", "                (None, None)\n")
        content = re.sub(
            r'(\.try_register_proof\([^;]+?,\s*&expires_at)\s*\)',
            r'\1, &None)',
            content
        )

    if "tests/ttl/src/fixture.rs" in path:
        content = re.sub(
            r'(\.register_proof\([^;]+?,\s*&expires_at)\s*\);',
            r'\1, &predecessor);',
            content
        )
        
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

paths = [
    "tests/events/src/harness.rs",
    "tests/cross-contract/src/harness.rs",
    "tests/emergency/src/harness.rs",
    "tests/property/state_machine.rs",
    "tests/ttl/src/proof_registry_ttl.rs",
    "tests/authorization/src/harness.rs",
    "tests/ttl/src/fixture.rs",
]

for p in paths:
    fix(p)

print("Fixed specifically.")
