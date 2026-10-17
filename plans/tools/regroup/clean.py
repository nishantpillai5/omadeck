"""Comment cleanup for upstream, applied to every tree of the up/* series by build.py.

Works on whole comment blocks: joins each paragraph, rewrites a few phrases, drops
asides and sentences that point at the private plans (PR numbers, BUGS.md, tasks) or at
one machine (captures, dates), and re-wraps only the paragraphs it changed. A block left
empty is removed. Trailing comments that point at the plans are dropped.

Preview: clean.py <file>... rewrites the files in place."""
import re, sys, textwrap

# On a paragraph's joined text, before anything is dropped.
SUBS = [
    (re.compile(r"Real output captured on an Omarchy machine\."), "Real pactl output."),
    (re.compile(r"Captured on the Omarchy machine, [\d-]+ \([^)]*\): a"), "Recorded from playerctl: a"),
    (re.compile(r"The positions are a snapshot of the capture\."), "The positions are a snapshot."),
    (re.compile(r"are captured on the machine \(see plans/[^)]*\): each"), "were checked on Omarchy: each"),
    (re.compile(r"from the commands captured on the machine \(plans/[^)]*\)"), "from commands checked on Omarchy"),
    (re.compile(r"A transcript captured on the Omarchy machine \([^)]*?(Hyprland [\d.]+)\):"), r"A recorded transcript (\1):"),
    (re.compile(r"BUGS\.md \d+'s path"), "the held-Back reload path"),
    (re.compile(r", as PR \d+'s "), ", as "),
    (re.compile(r"(?:BUGS\.md|[Bb]ugs?) \d+(?:, \d+)*: (\w)"), lambda m: m.group(1).upper()),
    (re.compile(r"\s*\((?:BUGS\.md|[Bb]ugs?) \d+(?:, \d+)*\)"), ""),
    (re.compile(r"\s*\([^()]*\b(?:PRs?|PR) ?\d+[^()]*\)"), ""),
    (re.compile(r"\s*\([^()]*captured yet\)"), ""),
    (re.compile(r"\s*\([^()]*fix/review[^()]*\)"), ""),
    (re.compile(r"Groundwork's pattern for panels, for keys"), "As for panels, for keys"),
    (re.compile(r"\s*\((?:D|R)\d+\)"), ""),
    (re.compile(r"\s*\([^()]*\bthis PR\b[^()]*\)"), ""),
    (re.compile(r"\s*\([^()]*(?:pr\d+-[a-z]|[Cc]apture \d|the plan's|to check on the machine|capturing machine)[^()]*\)"), ""),
]
BANNED = re.compile(r"\bthis PR\b|may use it later|\bPRs? ?\d+|Later PRs|plans/|\bpr\d+-[a-z]|\b[Tt]ask \d|\b(?:the|an Omarchy|capturing) machine\b(?!'s)|\b20\d\d-\d\d-\d\d\b(?!T)|BUGS\.md|[Cc]apture \d")
SENT_SPLIT = re.compile(r"(?<!e\.g\.)(?<!i\.e\.)(?<=[.!?])\s+(?=[A-Z`(\[*_$\"'])")

COMMENT = re.compile(r"^(\s*)(///|//!|//)(?: (.*)|())$")
ITEM = re.compile(r"^(?:[-*] |\d+\. )")

def clean_text(t):
    for rx, b in SUBS:
        t = rx.sub(b, t)
    if not BANNED.search(t):
        return t
    keep = [s for s in SENT_SPLIT.split(t) if not BANNED.search(s)]
    return " ".join(keep)

def wrap(text, first, cont, width):
    return textwrap.wrap(text, width=width, initial_indent=first, subsequent_indent=cont,
                         break_long_words=False, break_on_hyphens=False) or []

def clean_block(lines):
    """lines: the raw lines of one comment block (same indent and marker)."""
    m = COMMENT.match(lines[0])
    indent, mark = m.group(1), m.group(2)
    body = [COMMENT.match(l).group(3) or "" for l in lines]
    if any("```" in b for b in body):
        return lines
    width = 92
    # paragraphs: (item_prefix, [text lines]) split at blank lines and list items
    paras, cur = [], None
    for b in body:
        if not b.strip():
            if cur:
                paras.append(cur)
            paras.append(None)
            cur = None
        elif ITEM.match(b) or cur is None or (cur[0] == "" and b.startswith("  ") and not cur[1][-1].startswith("  ")):
            if cur:
                paras.append(cur)
            im = ITEM.match(b)
            cur = (im.group(0) if im else "", [b[len(im.group(0)):] if im else b])
        else:
            cur[1].append(b.strip() if cur[0] else b)
    if cur:
        paras.append(cur)
    out, changed = [], False
    for p in paras:
        if p is None:
            out.append(None)
            continue
        item, tl = p
        if not item and len(tl) > 1 and any(t.startswith("  ") for t in tl):
            # pre-formatted (aligned) text: clean line by line, never reflow
            new = [clean_text(t) for t in tl]
            if new != tl:
                changed = True
            out.append(("raw", item, [n for n in new if n.strip()]))
            continue
        joined = " ".join(t.strip() for t in tl)
        new = clean_text(joined).strip()
        if new != joined:
            changed = True
            out.append(("wrap", item, new))
        else:
            out.append(("keep", item, tl))
    if not changed:
        return lines
    res = []
    prefix = f"{indent}{mark} "
    for o in out:
        if o is None:
            res.append(None)
            continue
        kind, item, v = o
        if kind == "keep":
            res.append([prefix + item + v[0]] + [prefix + (" " * len(item) if item else "") + t.strip() if item else prefix + t for t in v[1:]])
        elif kind == "raw":
            if v:
                res.append([prefix + t for t in v])
        elif v:
            res.append(wrap(v, prefix + item, prefix + " " * len(item), width))
    # drop blank separators at the ends and doubled ones
    flat, prev_blank = [], True
    for r in res:
        if r is None:
            if not prev_blank:
                flat.append(None)
            prev_blank = True
        else:
            flat.extend(r)
            prev_blank = False
    while flat and flat[-1] is None:
        flat.pop()
    return [f"{indent}{mark}" if l is None else l.rstrip() for l in flat]

TRAIL = re.compile(r"^(.*?\S)(\s+)// (.*)$")

def clean_trailing(line):
    m = TRAIL.match(line)
    if not m or m.group(1).count('"') % 2 or line.lstrip().startswith("//"):
        return line
    code, sp, text = m.groups()
    t = text
    for rx, b in SUBS:
        t = rx.sub(b, t)
    if BANNED.search(t) or not t.strip():
        return code
    return line if t == text else f"{code}{sp}// {t}"

def clean(src):
    lines = src.split("\n")
    out, i = [], 0
    while i < len(lines):
        m = COMMENT.match(lines[i])
        if m:
            j = i
            while j + 1 < len(lines):
                n = COMMENT.match(lines[j + 1])
                if not n or n.group(1) != m.group(1) or n.group(2) != m.group(2):
                    break
                j += 1
            out.extend(clean_block(lines[i:j + 1]))
            i = j + 1
        else:
            out.append(clean_trailing(lines[i]))
            i += 1
    return "\n".join(out)

if __name__ == "__main__":
    for p in sys.argv[1:]:
        s = open(p).read()
        t = clean(s)
        if t != s:
            open(p, "w").write(t)
