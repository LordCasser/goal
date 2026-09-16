#!/usr/bin/env python3
"""Extract structural evidence from the minified SolidJS bundles:
   - HTML template literals (component skeletons, class names, data attributes)
   - user-visible copy strings
   - aria/role/title attributes
"""
import re
import glob
import sys
import os

BUNDLES = sorted(glob.glob(os.path.join(os.path.dirname(__file__), '..', 'evidence', 'web', '_assets_*.js')))
OUTDIR = os.path.join(os.path.dirname(__file__), '..', 'evidence', 'ui')
os.makedirs(OUTDIR, exist_ok=True)

TEMPLATE = re.compile(r'`(<[^`]{40,})`')
COPY = re.compile(r'''["'`]([A-Z][^"'`\n]{6,200}?)["'`]''')


def main():
    all_templates = []
    all_copy = []
    for f in BUNDLES:
        name = os.path.basename(f)
        txt = open(f, encoding='utf-8', errors='ignore').read()
        for m in TEMPLATE.finditer(txt):
            html = m.group(1)
            if 'class=' not in html and 'class="' not in html:
                continue
            all_templates.append((name, html))
        for m in COPY.finditer(txt):
            s = m.group(1)
            if re.search(r'[A-Za-z] [a-z]', s) or s.endswith(('.', '?', '!')):
                all_copy.append((name, s))

    with open(os.path.join(OUTDIR, 'templates.html'), 'w') as fh:
        cur = None
        for name, html in all_templates:
            if name != cur:
                fh.write(f'\n\n<!-- ============ {name} ============ -->\n')
                cur = name
            fh.write(html + '\n')
    with open(os.path.join(OUTDIR, 'copy.txt'), 'w') as fh:
        cur = None
        for name, s in sorted(all_copy):
            if name != cur:
                fh.write(f'\n### {name}\n')
                cur = name
            fh.write(s + '\n')
    print(f'templates: {len(all_templates)}  copy strings: {len(all_copy)}')


if __name__ == '__main__':
    main()
