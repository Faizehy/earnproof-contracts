import os
import re

def fix_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # 1. Update tests fixture register_proof signature
    content = re.sub(
        r'pub fn register_proof\(&self, expires_at: u64\) -> BytesN<32> \{',
        'pub fn register_proof(&self, expires_at: u64, predecessor: Option<BytesN<32>>) -> BytesN<32> {',
        content
    )
    
    # 2. Update fixture's internal register_proof call
    # self.proofs.register_proof(..., &expires_at); -> self.proofs.register_proof(..., &expires_at, &predecessor);
    content = re.sub(
        r'(\.register_proof\([^;]+?)(,\s*&expires_at)\);',
        r'\1\2, &predecessor);',
        content
    )

    # 3. Update tests calling deployment.register_proof(expires_at) -> deployment.register_proof(expires_at, &None)
    # Actually Deployment uses Option directly, so deployment.register_proof(FAR_FUTURE) -> deployment.register_proof(FAR_FUTURE, None)
    content = re.sub(
        r'(\.register_proof\(\s*[A-Z_0-9a-z_]+?\s*)\)',
        r'\1, None)',
        content
    )

    # 4. Update manual register_proof calls using 5 arguments -> 6 arguments
    # register_proof(&proof_id_hash, &commitment, &issuer, &schema, &expires)
    content = re.sub(
        r'(\.register_proof\([^,]+?,\s*[^,]+?,\s*[^,]+?,\s*[^,]+?,\s*[^,)]+?)\)',
        r'\1, &None)',
        content
    )
    
    # Try another pass for ones split across multiple lines
    content = re.sub(
        r'(\.register_proof\(\s*&[^,]+?,\s*&[^,]+?,\s*&[^,]+?,\s*&[^,]+?,\s*&[^,)]+?)\s*\)',
        r'\1, &None)',
        content
    )

    # Note: `try_register_proof` in tests/ttl/src/missing_state.rs might also need an update if they use the raw client.
    content = re.sub(
        r'(\.try_register_proof\([^,]+?,\s*[^,]+?,\s*[^,]+?,\s*[^,]+?,\s*[^,)]+?)\)',
        r'\1, &None)',
        content
    )

    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(content)

for root, _, files in os.walk('.'):
    if 'target' in root: continue
    for file in files:
        if file.endswith('.rs'):
            fix_file(os.path.join(root, file))

print("Fixed arguments in tests.")
