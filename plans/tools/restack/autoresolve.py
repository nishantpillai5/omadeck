#!/usr/bin/env python3
"""Resolve conflict hunks where upstream's side is the common base plus a known upstream
edit: the resolution is the series' side with that same edit. Prints the files it left
with conflicts (exit 1), or nothing (exit 0).

Usage: autoresolve.py <file>..."""
import re, sys

# (base text, what upstream made of it), applied to the series' side.
EDITS = [
    ("use crate::paths;\n", "use crate::paths::{self, FileStamp};\n"),
]

# (anchor, lines upstream inserted after it): when upstream's side has them and the
# series' side has the anchor but not them, the resolution is the series' side with them.
INSERTS = [
    ("            font_pattern: None,\n",
     "            fonts_stamp: FileStamp::of(&paths::fontconfig_file()),\n            fallbacks: HashMap::new(),\n"),
]

HUNK = re.compile(r"<<<<<<< [^\n]*\n(.*?)\|\|\|\|\|\|\| [^\n]*\n(.*?)=======\n(.*?)>>>>>>> [^\n]*\n", re.S)

def resolve(m):
    head, base, theirs = m.group(1), m.group(2), m.group(3)
    # Both sides only added code here: keep both, upstream's first.
    if not base.strip():
        return head + theirs
    # Upstream only changed whitespace here: the series' side wins.
    if [l.split() for l in head.splitlines()] == [l.split() for l in base.splitlines()]:
        return theirs
    for anchor, ins in INSERTS:
        if anchor + ins in head and anchor in theirs and ins not in theirs:
            return theirs.replace(anchor, anchor + ins, 1)
    for a, b in EDITS:
        if a in base and base.replace(a, b) == head:
            return theirs.replace(a, b)
    return m.group(0)

left = []
for p in sys.argv[1:]:
    s = open(p).read()
    t = HUNK.sub(resolve, s)
    if t != s:
        open(p, "w").write(t)
    if "<<<<<<< " in t:
        left.append(p)
print("\n".join(left))
sys.exit(1 if left else 0)
