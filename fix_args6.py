import os

def fix_rust_files(root_dir):
    for dirpath, _, filenames in os.walk(root_dir):
        if 'target' in dirpath:
            continue
        for filename in filenames:
            if not filename.endswith('.rs'):
                continue
                
            path = os.path.join(dirpath, filename)
            with open(path, 'r', encoding='utf-8') as f:
                original = f.read()
                
            # 1. fix fixture.rs signatures
            content = original.replace(
                'pub fn register_proof(&self, expires_at: u64) -> BytesN<32> {',
                'pub fn register_proof(&self, expires_at: u64, predecessor: Option<BytesN<32>>) -> BytesN<32> {'
            )
            content = content.replace(
                'pub fn register_proof(&self, expires_at: u64, predecessor_id_hash: Option<BytesN<32>>) -> BytesN<32> {',
                'pub fn register_proof(&self, expires_at: u64, predecessor: Option<BytesN<32>>) -> BytesN<32> {'
            )
            
            # fix inner fixture
            content = content.replace(', &expires_at);', ', &expires_at, &predecessor);')
            content = content.replace(', &expires_at, &predecessor_id_hash);', ', &expires_at, &predecessor);')

            # 2. fix deployment.register_proof(
            def repl_dep(text):
                res = []
                idx = 0
                while True:
                    fnd = text.find('.register_proof(', idx)
                    if fnd == -1:
                        res.append(text[idx:])
                        break
                    # only apply to deployment
                    if 'deployment' not in text[max(0, fnd-30):fnd] and 'self.proofs' not in text[max(0, fnd-30):fnd]:
                        res.append(text[idx:fnd + 16])
                        idx = fnd + 16
                        continue
                        
                    res.append(text[idx:fnd + 16])
                    idx = fnd + 16
                    
                    # find matching paren
                    pc = 1
                    curr = idx
                    while curr < len(text) and pc > 0:
                        if text[curr] == '(': pc += 1
                        elif text[curr] == ')': pc -= 1
                        curr += 1
                        
                    args = text[idx:curr-1]
                    if ',' not in args:
                        res.append(args + ", None")
                        res.append(")")
                        idx = curr
                    else:
                        res.append(args)
                        res.append(")")
                        idx = curr
                return "".join(res)
                
            content = repl_dep(content)
            
            # 3. fix client.try_register_proof and client.register_proof
            def repl_client(text):
                res = []
                idx = 0
                while True:
                    f1 = text.find('.register_proof(', idx)
                    f2 = text.find('.try_register_proof(', idx)
                    if f1 == -1 and f2 == -1:
                        res.append(text[idx:])
                        break
                    
                    if f1 != -1 and (f2 == -1 or f1 < f2):
                        start = f1 + 16
                    else:
                        start = f2 + 20
                        
                    # Skip if it's deployment.register_proof (handled above)
                    is_dep = 'deployment' in text[max(0, start-40):start]
                    
                    res.append(text[idx:start])
                    idx = start
                    
                    pc = 1
                    curr = idx
                    while curr < len(text) and pc > 0:
                        if text[curr] == '(': pc += 1
                        elif text[curr] == ')': pc -= 1
                        curr += 1
                        
                    args = text[idx:curr-1]
                    
                    # check commas at top level
                    commas = 0
                    d = 0
                    for c in args:
                        if c == '(': d += 1
                        elif c == ')': d -= 1
                        elif c == ',' and d == 0: commas += 1
                        
                    if not is_dep:
                        if commas == 4:
                            if args.rstrip().endswith(','):
                                res.append(args + " &None")
                            else:
                                res.append(args + ", &None")
                        elif commas == 5:
                            if "predecessor_id_hash" in args:
                                cleaned = args.replace("&predecessor_id_hash, &None", "&None")
                                res.append(cleaned)
                            elif args.rstrip().endswith(','):
                                # It's a 5 argument call with trailing comma!
                                # e.g. "a, b, c, d, e,"
                                res.append(args + " &None,")
                            else:
                                res.append(args)
                        elif commas == 6:
                            cleaned = args.replace("&predecessor_id_hash, &None", "&None")
                            cleaned = cleaned.replace(", &None, &None", ", &None")
                            res.append(cleaned)
                        else:
                            res.append(args)
                    else:
                        res.append(args)
                        
                    res.append(")")
                    idx = curr
                return "".join(res)
                
            content = repl_client(content)
            
            # Clean up double Nones
            content = content.replace("None, None", "None")
            content = content.replace("&None, &None", "&None")
            content = content.replace(", &None, &None", ", &None")
            content = content.replace(" &None,,", " &None,")
            
            if content != original:
                with open(path, 'w', encoding='utf-8') as f:
                    f.write(content)

if __name__ == '__main__':
    fix_rust_files('.')
    print("Fixed.")
