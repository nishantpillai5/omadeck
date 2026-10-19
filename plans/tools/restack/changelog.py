#!/usr/bin/env python3
"""CHANGELOG for a commit restacked on upstream's release: upstream's file, with the
commit's own entries (its Unreleased bullets that aren't upstream's) under Unreleased.

Usage: changelog.py <original commit> <upstream ref>  -> prints the file."""
import re, subprocess, sys

def show(ref, path="CHANGELOG.md"):
    r = subprocess.run(["git", "show", f"{ref}:{path}"], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else ""

def unreleased(text):
    i = text.find("## [Unreleased]")
    if i < 0:
        return ""
    j = text.find("\n## [", i + 1)
    return text[i + len("## [Unreleased]"):j if j >= 0 else len(text)]

def sections(block):
    """[(name, [bullet text])] in order; a bullet is a "- " line plus its indented lines."""
    out, cur = [], None
    for line in block.splitlines():
        m = re.match(r"### (.+)", line)
        if m:
            cur = (m.group(1).strip(), [])
            out.append(cur)
        elif line.startswith("- ") and cur:
            cur[1].append(line + "\n")
        elif line.startswith("  ") and cur and cur[1]:
            cur[1][-1] += line + "\n"
    return out

def norm(b):
    return " ".join(b.split())

def main():
    orig, up = sys.argv[1], sys.argv[2]
    upstream = show(up)
    released = {norm(b) for _, bs in sections(upstream[upstream.find("## [0."):]) for b in bs}
    ours = [(n, [b for b in bs if norm(b) not in released]) for n, bs in sections(unreleased(show(orig)))]
    body = "".join(f"\n### {n}\n" + "".join(bs) for n, bs in ours if bs)
    head = "## [Unreleased]\n"
    i = upstream.index(head) + len(head)
    j = upstream.index("\n## [", i - 1)
    sys.stdout.write(upstream[:i] + body + "\n" + upstream[j + 1:])

main()
