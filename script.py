from pathlib import Path
p=Path('src-tauri/src/skills.rs')
s=p.read_text()
print(repr(s[:100]))
