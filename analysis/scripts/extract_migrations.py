#!/usr/bin/env python3
"""Extract every migration's SQL verbatim from the hyperfocus binary.

sqlx embeds each migration as `sqlx::migrate!` string data. In the compiled
binary the migration *description* is immediately followed by its SQL body, so
we locate each description and dump the window that follows, then cut at the
next description.
"""
import os
import re
import json

BIN = '/Volumes/hyperfocus/hyperfocus.app/Contents/MacOS/hyperfocus'
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'evidence', 'db', 'migrations-sql.txt')

MIGRATIONS = [
    (1, "initial schema"), (2, "onboarding data"), (3, "drop checkins table"),
    (4, "drop plan reality rating"), (5, "add pro id to periods"),
    (6, "convert focused time to milliseconds"), (7, "add scores table"),
    (8, "add clarity data to tasks"), (9, "update pro ids"),
    (10, "add parent id and clear clarity"), (11, "onboarding clarity"),
    (12, "clarity breakdown migration"), (13, "goal evaluation schema"),
    (14, "reset clarity breakdown for granular issues"),
    (15, "replace execution clarity with structural type"), (16, "remove pro columns"),
    (17, "add goal breakdown"), (18, "add is refined to tasks"),
    (19, "drop migrated from task id"), (20, "onboarding is refined"),
    (21, "drop clarity breakdown"), (22, "add active to periods"),
    (23, "seed later periods"), (24, "drop onboarding tasks for fresh installs"),
    (25, "collapse later periods"), (26, "add prioritization breakdown"),
    (27, "extend onboarding day to 24 hours"), (28, "add preview and revision metadata"),
    (29, "add task tombstones"), (30, "drop task preview state"),
    (31, "drop revision metadata"), (32, "add agent conversation persistence"),
    (33, "replace tombstones with agent proposals"), (34, "agent conversation task fk set null"),
    (35, "update onboarding month to 28 days"), (36, "cleanup untouched onboarding children"),
    (37, "add agent conversation active skill"), (38, "add tasks to repeats"),
    (39, "update onboarding period labels for activation"), (40, "rename periods to cycles"),
    (41, "add task link metadata"), (42, "add preview original root color key"),
    (43, "add task copy lineage"), (44, "remove untouched onboarding month"),
    (45, "allow null cycle duration"), (46, "update agent conversation active skill constraint"),
    (47, "add dated cycle fields"), (48, "drop scores table"),
    (49, "add clarity flags to tasks"), (50, "add clarity flags to preview originals"),
    (51, "drop legacy clarity columns"), (52, "clear stale goal breakdown state"),
    (53, "reset agent conversations for launch actions"), (54, "drop cycle active"),
    (55, "drop repeat tasks"), (56, "add planning issue dismissals"),
    (57, "add cycle deletion cascades"),
]

SQL_HINT = re.compile(rb'^\s*(?:--|CREATE|ALTER|DROP|UPDATE|INSERT|DELETE|PRAGMA|\()')


def main():
    data = open(BIN, 'rb').read()

    # Locate every place each description occurs; pick the occurrence followed by SQL.
    located = []
    for version, desc in MIGRATIONS:
        needle = desc.encode()
        chosen = None
        start = 0
        while True:
            i = data.find(needle, start)
            if i < 0:
                break
            after = data[i + len(needle):i + len(needle) + 40]
            if SQL_HINT.match(after):
                chosen = i
                break
            start = i + 1
        located.append((version, desc, chosen))

    found = sum(1 for _, _, c in located if c is not None)
    print(f"located {found}/{len(MIGRATIONS)} migration bodies")

    lines = ["# hyperfocus 0.15.0 — migration SQL (verbatim from binary)", ""]
    bounds = [c for _, _, c in located if c is not None]
    for idx, (version, desc, off) in enumerate(located):
        lines.append(f"\n{'=' * 78}\n## {version:02d}. {desc}\n{'=' * 78}")
        if off is None:
            lines.append("(body not located — description occurs only in metadata)")
            continue
        # cut at the next located migration body, or a generous fallback
        nxt = None
        for _, _, c in located[idx + 1:]:
            if c is not None and c > off:
                nxt = c
                break
        end = nxt if nxt else off + 6000
        raw = data[off + len(desc):end]
        text = raw.decode('utf-8', 'replace')
        # keep only printable-ish content, stop at first long non-text run
        cut = re.search(r'[\x00-\x08\x0b\x0c\x0e-\x1f]{1,}', text)
        if cut:
            text = text[:cut.start()]
        lines.append(text.strip())

    with open(OUT, 'w') as fh:
        fh.write('\n'.join(lines))
    print(f"wrote {OUT}")
    print(f"size: {os.path.getsize(OUT)} bytes")


if __name__ == '__main__':
    main()
