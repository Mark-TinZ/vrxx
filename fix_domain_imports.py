import os

directory = 'src'
replacements = [
    ('crate::key_parser::', 'crate::domain::key_parser::'),
    ('crate::xray_config::', 'crate::domain::xray_config::'),
    ('crate::singbox_config::', 'crate::domain::singbox_config::'),
    ('use crate::key_parser::', 'use crate::domain::key_parser::'),
    ('use crate::xray_config::', 'use crate::domain::xray_config::'),
    ('use crate::singbox_config::', 'use crate::domain::singbox_config::'),
]

for root, dirs, files in os.walk(directory):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r', encoding='utf-8') as f:
                content = f.read()

            new_content = content
            for old, new in replacements:
                new_content = new_content.replace(old, new)
            
            if new_content != content:
                with open(path, 'w', encoding='utf-8') as f:
                    f.write(new_content)
