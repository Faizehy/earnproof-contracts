import os

def fix_args(content):
    idx = 0
    while True:
        # Find the next occurrence
        found_idx = content.find('.register_proof(', idx)
        found_try = content.find('.try_register_proof(', idx)
        
        if found_idx == -1 and found_try == -1:
            break
            
        if found_idx != -1 and (found_try == -1 or found_idx < found_try):
            start = found_idx + len('.register_proof(')
        else:
            start = found_try + len('.try_register_proof(')
            
        # Find matching parenthesis
        paren_count = 1
        curr = start
        while curr < len(content) and paren_count > 0:
            if content[curr] == '(':
                paren_count += 1
            elif content[curr] == ')':
                paren_count -= 1
            curr += 1
            
        end = curr - 1
        
        # Check if we already have 6 arguments (5 commas at the top level)
        args_text = content[start:end]
        
        # Count top-level commas
        commas = 0
        depth = 0
        for char in args_text:
            if char == '(': depth += 1
            elif char == ')': depth -= 1
            elif char == ',' and depth == 0: commas += 1
            
        if commas == 4:
            # We have 5 arguments, add the 6th
            # Check if this is a deployment call which doesn't use reference, or a client call which does
            is_deployment = "deployment" in content[max(0, start-40):start]
            
            replacement = ", &None"
            # If the call is something like `client.register_proof(..., &expires)`, it usually has `&` for args.
            
            content = content[:end] + replacement + content[end:]
            
        elif commas == 0 and "expires_at" in args_text:
            # It's deployment.register_proof(expires_at)
            content = content[:end] + ", None" + content[end:]
            
        idx = end + 1

    return content

for root, _, files in os.walk('.'):
    if 'target' in root: continue
    for file in files:
        if file.endswith('.rs'):
            filepath = os.path.join(root, file)
            with open(filepath, 'r', encoding='utf-8') as f:
                content = f.read()
            
            # 1. Update tests fixture register_proof signature
            content = content.replace(
                'pub fn register_proof(&self, expires_at: u64) -> BytesN<32> {',
                'pub fn register_proof(&self, expires_at: u64, predecessor_id_hash: Option<BytesN<32>>) -> BytesN<32> {'
            )
            # update internal fixture calls
            content = content.replace(', &expires_at);', ', &expires_at, &predecessor_id_hash);')
            
            new_content = fix_args(content)
            
            if new_content != content:
                with open(filepath, 'w', encoding='utf-8') as f:
                    f.write(new_content)

print("Fixed using parser.")
