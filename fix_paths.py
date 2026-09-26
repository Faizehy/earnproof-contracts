import sys

path = 'tests/compatibility/mod.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace('"tests/compatibility/goldens', '"goldens')

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)
