#!/usr/bin/env python3
"""Reconstruct the Tauri command registry (authoritative, from the binary).

`tauri::generate_handler!` emits registered command names as concatenated Rust
string literals inside a band of `__TEXT,__const`.

Greedy longest-match fails here: the band also contains fragments that span two
adjacent command names, so "longest" merges them. We instead run a DP that
maximises the number of tokens, over a vocabulary restricted to verb-initial
multi-word identifiers. Splitting into more valid names is exactly what we want.
"""
import json
import os
import re
from functools import lru_cache

BIN = '/Volumes/hyperfocus/hyperfocus.app/Contents/MacOS/hyperfocus'
BAND = (18_718_000, 18_762_100)
HERE = os.path.dirname(os.path.abspath(__file__))

VERBS = {
    'get', 'set', 'add', 'update', 'delete', 'create', 'copy', 'move', 'keep',
    'undo', 'start', 'finish', 'send', 'submit', 'dismiss', 'skip', 'save',
    'remove', 'connect', 'cancel', 'reorder', 'reset', 'prepare', 'play',
    'show', 'mark', 'acknowledge', 'continue', 'exit', 'record', 'close',
    'reconcile', 'stop', 'select', 'switch',
}

data = open(BIN, 'rb').read()
band = data[BAND[0]:BAND[1]]
n = len(band)

vocab = {m.group(0) for m in re.finditer(rb'[a-z][a-z0-9_]{7,64}', band)}
vocab = {
    w for w in vocab
    if b'_' in w and w.split(b'_')[0].decode() in VERBS
}
by_first = {}
for w in sorted(vocab, key=len, reverse=True):
    by_first.setdefault(w[0], []).append(w)

# DP over the band, from the end backwards:
#   best[i] = (token_count, tokens) for band[i:], maximising token_count.
# Skipping a byte costs nothing, so the DP picks the densest set of
# non-overlapping valid names — which is exactly the split we want, because a
# token spanning two adjacent names yields 1 while the correct split yields 2.
best = [(0, [])] * (n + 1)
for i in range(n - 1, -1, -1):
    count, tokens = best[i + 1]
    for w in by_first.get(band[i], ()):
        if band.startswith(w, i):
            sub = best[i + len(w)]
            if sub[0] + 1 > count:
                count, tokens = sub[0] + 1, [w.decode()] + sub[1]
    best[i] = (count, tokens)

commands, seen = [], set()
for name in best[0][1]:
    if name not in seen:
        seen.add(name)
        commands.append(name)

print(f"# registry-recovered commands: {len(commands)}\n")
for i, c in enumerate(commands, 1):
    print(f"{i:>3}. {c}")
json.dump(commands, open(os.path.join(HERE, 'registry.json'), 'w'), indent=2)
