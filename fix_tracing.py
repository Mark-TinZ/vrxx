import os
import re

directory = 'src'
for root, dirs, files in os.walk(directory):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r', encoding='utf-8') as f:
                content = f.read()

            # tracing::info!(&format!("...", args)); -> tracing::info!("...", args);
            # We can simplify by just regex replacing &format!(...)
            # Actually simpler: tracing::info!(&format!("...")) -> tracing::info!("{}", &format!("..."))
            
            def repl(m):
                level = m.group(1)
                inner = m.group(2)
                return f'tracing::{level}!("{{}}", {inner})'

            new_content = re.sub(r'tracing::(info|debug|warn|error)!\(\s*(&format!\(.*?\))\s*\)', repl, content)
            
            if new_content != content:
                with open(path, 'w', encoding='utf-8') as f:
                    f.write(new_content)
