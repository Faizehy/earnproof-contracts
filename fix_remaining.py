import re

def fix(path):
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    # for harness files
    if "harness.rs" in path:
        # replace `self.proofs.register_proof(..., &expires_at);` or similar
        content = re.sub(
            r'(\.register_proof\([^;]+?,\s*&[a-z_().0-9]+?)(?:\s*, \/\/[^\n]*)?\s*\)',
            r'\1, &None)',
            content
        )
        content = re.sub(
            r'(\.try_register_proof\([^;]+?,\s*&[a-z_().0-9]+?)\s*\)',
            r'\1, &None)',
            content
        )
    
    if "matrix.rs" in path or "rotation.rs" in path:
        content = content.replace(', None)', ')')
        
    if "proof-registry/src/lib.rs" in path:
        content = content.replace("&current_time, // Equal to current time, must be rejected\n            );", "&current_time, // Equal to current time, must be rejected\n                &None,\n            );")
        
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

fix("tests/cross-contract/src/harness.rs")
fix("tests/authorization/src/harness.rs")
fix("tests/authorization/src/matrix.rs")
fix("tests/authorization/src/rotation.rs")
fix("contracts/proof-registry/src/lib.rs")

