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
    content = re.sub(
        r'(\.register_proof\([^;]+?)(,\s*&expires_at)\);',
        r'\1\2, &predecessor);',
        content
    )

    # 3. Update tests calling deployment.register_proof(expires_at)
    def repl_deployment(m):
        args = m.group(2)
        if 'None' in args or 'predecessor' in args: return m.group(0)
        return f"{m.group(1)}{args}, None)"

    content = re.sub(
        r'(\.register_proof\()([^;)]+)\)',
        repl_deployment,
        content
    )

    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(content)

for root, _, files in os.walk('.'):
    if 'target' in root: continue
    for file in files:
        if file.endswith('.rs'):
            fix_file(os.path.join(root, file))

# Need to fix specific files for raw client `.register_proof(` adding `&None`
def fix_raw_client(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
        
    def repl_raw(m):
        args = m.group(2)
        # If it has only 1 arg (like expires_at for deployment), don't touch here
        if args.count(',') < 3: return m.group(0)
        if '&None' in args or '&predecessor' in args: return m.group(0)
        return f"{m.group(1)}{args}, &None)"

    content = re.sub(
        r'(\.(?:try_)?register_proof\()([^;)]+)\)',
        repl_raw,
        content
    )
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(content)

for root, _, files in os.walk('.'):
    if 'target' in root: continue
    for file in files:
        if file.endswith('.rs'):
            fix_raw_client(os.path.join(root, file))

print("Fixed arguments in tests.")
