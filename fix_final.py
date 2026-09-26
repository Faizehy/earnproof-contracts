import os
import re

def fix(path):
    if not os.path.exists(path):
        return
        
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    if "harness.rs" in path:
        # replace `self.proofs.register_proof` or `self.proofs.try_register_proof`
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
        
        # fix (None) to (None, None)
        content = content.replace('                (None)\n', '                (None, None)\n')
    
    # fix deployment.register_proof with extra None
    content = content.replace(', None)', ')')
    content = content.replace(', None);', ');')
    
    # fix the specific one in proof-registry
    if "proof-registry" in path:
        content = content.replace("&current_time, // Equal to current time, must be rejected\n            );", "&current_time, // Equal to current time, must be rejected\n                &None,\n            );")

    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

for root, _, files in os.walk('tests'):
    if 'target' in root: continue
    for file in files:
        if file.endswith('.rs'):
            fix(os.path.join(root, file))

fix('contracts/proof-registry/src/lib.rs')

print("Fixed final.")
