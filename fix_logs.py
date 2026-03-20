import os
import re

directory = 'src'
for root, dirs, files in os.walk(directory):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r', encoding='utf-8') as f:
                content = f.read()

            # crate::backend::log_app_event("info", "message"); -> tracing::info!("message");
            # log_app_event("info", "message"); -> tracing::info!("message");
            def repl(m):
                level = m.group(1).lower()
                msg = m.group(2)
                return f'tracing::{level}!({msg})'

            new_content = re.sub(r'(?:crate::backend::)?log_app_event\s*\(\s*"([^"]+)"\s*,\s*(.*?)\s*\)', repl, content)
            
            if new_content != content:
                with open(path, 'w', encoding='utf-8') as f:
                    f.write(new_content)
