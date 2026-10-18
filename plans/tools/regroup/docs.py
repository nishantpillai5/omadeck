"""Doc fixes from the final review, applied to every tree by build.py: the CHANGELOG
files each change where it belongs (behaviour changes under Changed), drops entries that
describe fixes to the series' own new features, gains the groundwork's user-visible
changes, and the media docs say when a command goes to playerctl directly."""

GROUNDWORK = 5          # group index of up/06-groundwork
GROUNDWORK_LAST = 11    # its last commit, where its changelog lines land

DROP = [
    # Upstream never had this bug: it is about dials, which this series adds.
    """- Replugging a smaller deck no longer leaves the configurator editing a key or dial the
  hardware doesn't have.
""",
    # A fix to the volume dial's own unreleased bars.
    """- Volume and mic bars on the strip now update optimistically: a volume action moves
  the bar immediately, and the daemon's read-back a moment later corrects it. A
  config reload no longer blanks the bars of devices that are still shown.
""",
]

# (text in Added, what stays in Added, the Changed line it becomes)
SPLIT = [
    ("""- Hold a Back key for half a second to go straight Home, as in the Elgato app. A Back key
  now goes back when you let go of it. In the configurator, double-click a folder key to
  open it or a Back key to go up; the page switcher shows a folder with its path
  (`Music › Media › Spotify`).
""", """- Hold a Back key for half a second to go straight Home, as in the Elgato app. In the
  configurator, double-click a folder key to open it or a Back key to go up; the page
  switcher shows a folder with its path (`Music › Media › Spotify`).
""", """- A Back key goes back when you let go of it, not when you press it, so holding it can
  go Home.
"""),
    ("""- Stream Deck +: press and hold a dial's panel on the touch strip to run its
  `long_press` action (Elgato's "long touch"); a dial without one runs its press, as a
  tap does. The configurator's dial editor has a **Long press on the strip** group.
""", """- Stream Deck +: press and hold a dial's panel on the touch strip to run its
  `long_press` action (Elgato's "long touch"). The configurator's dial editor has a
  **Long press on the strip** group.
""", """- Stream Deck +: holding a dial's panel on the touch strip runs the dial's press when it
  has no `long_press`, as a tap does (it did nothing before).
"""),
]

GROUNDWORK_CHANGED = """- With the daemon running, the configurator's brightness slider goes through the daemon
  instead of writing `config.toml`, and its strip preview shows what the deck shows
  instead of reading the system itself. A configurator that stops reading the daemon's
  socket can no longer hold up the deck.
"""
GROUNDWORK_FIXED = """- `hyprctl` and `pactl` calls time out, so a stalled Hyprland or audio server can't hang
  the deck.
- A tapped strip panel's highlight ends on time.
"""

MEDIA_CHANGELOG = ("""Play/pause, next
  and previous go through Omarchy's media commands when it has them; seeking and the
  panel need `playerctl`.""", """Play/pause, next
  and previous go through Omarchy's media commands when it has them, unless a media panel
  or key shows a player: then they go to that player through `playerctl`. Seeking and the
  panel need `playerctl`.""")
MEDIA_README = ("""Play/pause, next and previous go through Omarchy's own media commands when they're there, so its pop-up shows as for your media keys.""",
                """Play/pause, next and previous go through Omarchy's own media commands when they're there, so its pop-up shows as for your media keys, unless a media dial or key shows a player: then they go to that player through `playerctl`.""")
MEDIA_EXAMPLE = ("""type = "media"               # play/pause, through Omarchy's media keys when it has them""",
                 """type = "media"               # play/pause, to the player the on face follows""")


def _section(text, name):
    """(start, end) of the bullets under `### name` in Unreleased, or None."""
    head = text.find("## [Unreleased]")
    nxt = text.find("\n## [", head + 1)
    nxt = len(text) if nxt < 0 else nxt
    h = text.find(f"\n### {name}\n", head, nxt)
    if h < 0:
        return None
    start = h + len(f"\n### {name}\n")
    end = text.find("\n### ", start, nxt)
    end = nxt if end < 0 else end
    return start, end


def _add_to(text, name, lines, top=False, after=None):
    """Add bullet `lines` to section `name` (at its end; at its top; or right after the
    bullet `after` when present), creating the section right after Added if missing."""
    sec = _section(text, name)
    if sec is None:
        at = _section(text, "Added")[1]
        before = text[:at].rstrip("\n") + "\n"
        return before + f"\n### {name}\n" + lines + "\n" + text[at:].lstrip("\n")
    start, end = sec
    if after and after in text[start:end]:
        at = text.index(after, start) + len(after)
        return text[:at] + lines + text[at:]
    if top:
        return text[:start] + lines + text[start:]
    before = text[:start] + text[start:end].rstrip("\n") + "\n" + lines
    after = text[end:].lstrip("\n")
    return before + ("\n" + after if after else "")


def changelog(text, gi, k):
    for d in DROP:
        text = text.replace(d, "")
    if gi > GROUNDWORK or (gi == GROUNDWORK and k >= GROUNDWORK_LAST):
        text = _add_to(text, "Changed", GROUNDWORK_CHANGED)
        text = _add_to(text, "Fixed", GROUNDWORK_FIXED, top=True,
                       after="- Hyprland actions never hold up the deck.\n")
    for whole, kept, changed in SPLIT:
        if whole in text:
            text = text.replace(whole, kept)
            text = _add_to(text, "Changed", changed, top=True)
    text = text.replace(*MEDIA_CHANGELOG)
    return text


def transform(path, text, gi, k):
    if path == "CHANGELOG.md":
        return changelog(text, gi, k)
    if path == "README.md":
        return text.replace(*MEDIA_README)
    if path == "examples/config.toml":
        return text.replace(*MEDIA_EXAMPLE)
    return text
