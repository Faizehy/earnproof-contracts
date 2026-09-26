import os

def fix_ttl(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
        
    content = content.replace("deployment.register_proof(FAR_FUTURE)", "deployment.register_proof(FAR_FUTURE, None)")
    content = content.replace("deployment.register_proof(2_000)", "deployment.register_proof(2_000, None)")
    
    # also fix the 6 -> 5 argument issue in proof_registry_ttl.rs that was shown earlier?
    # wait, the previous cargo output said:
    # `error[E0061]: this method takes 6 arguments but 7 arguments were supplied`
    # because my replace changed `&expires_at` -> `&expires_at, &predecessor` BUT then added `&None`.
    # Let's fix that.
    content = content.replace(", &predecessor, &None);", ", &predecessor);")
    content = content.replace(", &predecessor_id_hash, &None);", ", &predecessor_id_hash);")
    content = content.replace(", &None, &None);", ", &None);")
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(content)

for root, _, files in os.walk('tests/ttl'):
    for file in files:
        if file.endswith('.rs'):
            fix_ttl(os.path.join(root, file))
print("Fixed ttl tests.")
