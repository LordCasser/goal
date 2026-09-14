#!/usr/bin/env python3
"""Extract the IPC contract from the recovered frontend bundles.

The app's client module calls the Rust side through one wrapper defined as

    async function x(e, t = {}, n) { return window.__TAURI_INTERNALS__.invoke(e, t, n) }

and the call sites look like `D(`command_name`, { param: value })`. So a
function call whose first argument is a backtick string literal is an IPC call;
bare snake_case literals (enum values, event names, field names) are not.
"""
import re
import glob
import os
import json

WEB = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'evidence', 'web')
CALL = re.compile(r'([A-Za-z_$][\w$]*)\(\s*`([a-z][a-z0-9]*_[a-z0-9_]{1,64})`\s*([,)])')
EVENT = re.compile(r'`((?:cycle|task|agent|goal|proposals)[a-z]*:[a-z_]+)`')

# Every IPC command name is snake_case; single-word matches are DOM or
# ProseMirror calls (`createElement(\`div\`)`, `dispatch(\`focus\`)`).
def is_ipc_command(name: str) -> bool:
    return '_' in name and not name.startswith('plugin_')


def balanced_object(text, start):
    depth = 0
    i = start
    in_str = None
    while i < len(text):
        c = text[i]
        if in_str:
            if c == '\\':
                i += 2
                continue
            if c == in_str:
                in_str = None
        elif c in '"\'`':
            in_str = c
        elif c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
            if depth == 0:
                return text[start:i + 1]
        i += 1
    return None


def top_level_keys(obj_src):
    keys, depth, i, in_str, buf = [], 0, 1, None, ''
    while i < len(obj_src) - 1:
        c = obj_src[i]
        if in_str:
            if c == '\\':
                buf += obj_src[i:i + 2]
                i += 2
                continue
            if c == in_str:
                in_str = None
            buf += c
        elif c in '"\'`':
            in_str = c
            buf += c
        elif c in '{[(':
            depth += 1
        elif c in '}])':
            depth -= 1
        elif depth == 0 and c == ':':
            m = re.search(r'([A-Za-z_$][\w$]*)\s*$', buf)
            if m:
                keys.append(m.group(1))
            buf = ''
        elif depth == 0 and c == ',':
            buf = ''
        else:
            buf += c
        i += 1
    return keys


def main():
    found = {}
    events = set()
    for path in sorted(glob.glob(os.path.join(WEB, '_assets_*.js'))):
        txt = open(path, encoding='utf-8', errors='ignore').read()
        for m in CALL.finditer(txt):
            name = m.group(2)
            if not is_ipc_command(name):
                continue
            params = set()
            if m.group(3) == ',' and txt[m.end():m.end() + 1].strip() == '{':
                obj = balanced_object(txt, txt.index('{', m.end() - 1))
                if obj:
                    params.update(top_level_keys(obj))
            entry = found.setdefault(name, {'params': set(), 'aliases': set(), 'files': set()})
            entry['params'].update(params)
            entry['aliases'].add(m.group(1))
            entry['files'].add(os.path.basename(path))
        for m in EVENT.finditer(txt):
            events.add(m.group(1))

    print(f"# IPC commands recovered from call sites: {len(found)}\n")
    for name in sorted(found):
        params = sorted(found[name]['params'])
        print(f"{name:<52} ({', '.join(params) if params else '—'})")
    print(f"\n# events: {len(events)}")
    for e in sorted(events):
        print(f"  {e}")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'ipc-extract.json')
    with open(out, 'w') as fh:
        json.dump({
            'commands': {k: sorted(v['params']) for k, v in sorted(found.items())},
            'events': sorted(events),
        }, fh, indent=2, ensure_ascii=False)
    print(f"\nwrote {out}")


if __name__ == '__main__':
    main()
