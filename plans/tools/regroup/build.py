#!/usr/bin/env python3
"""Rebuild the up/* series from up/work: fold each group's fix commits into its task
commits by tree (no cherry-picks, so no conflicts), scrub personal strings from the
committed fixtures, and write new messages.

Rule: a file a fix touched gets its group-final content in the last new commit of the
group whose tasks touched it; a file no task touched gets it in the new commit holding
the last task before the last fix that touched it. OVERRIDE moves a file later when an
earlier placement doesn't build. Each group's last new commit has exactly the group's
final tree (the verified one), scrubs aside.
"""
import json, os, re, subprocess, sys, tempfile
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from clean import clean
from fixes import FIXES

TRAILER = "\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n"
BASE = "fb9052f"          # upstream/pages-and-folders
WORK = "up/work"

def git(*args, input=None, env=None):
    e = dict(os.environ)
    if env:
        e.update(env)
    r = subprocess.run(["git", *args], input=input, capture_output=True, env=e)
    if r.returncode != 0:
        sys.exit(f"git {' '.join(args)} failed: {r.stderr.decode()}")
    return r.stdout

def out(*args, **kw):
    return git(*args, **kw).decode().strip()

SCRUB = {
    "crates/omadeckd/tests/fixtures/socket2.txt": [
        ("Kapil का Naughty Humour है लाजवाब | Comedy Circus | Most Viewed - YouTube - Brave",
         "Ünïcode tïtle — 例え | Part 1 | Clips - Video - Brave"),
        ("Loose Talk Episode 251 [Subtitle Eng] | Moin Akhtar | Anwar Maqsood | ARY Digital - YouTube - Brave",
         "Talk Show Episode 12 [Subtitles] | Host One | Host Two - YouTube - Brave"),
        ("The MCU since you stopped watching - YouTube - Brave", "A Video, With a Comma - YouTube - Brave"),
        ("noobaf@omarchy:~", "user@host:~"),
        ("omarchy:Work", "shell"),
        ("hl-virtual-keyboard-fcitx5,English (US)", "keyboard,English (US)"),
    ],
    "crates/omadeckd/src/hypr.rs": [("omarchy:Work", "shell")],
}
FILM = [
    ("Bhagam Bhag 2006 (HD) - Full Movie - Superhit Comedy Movie - Akshay Kumar - Govinda -  Paresh Rawal",
     "Example Film (HD) - Full Movie - Comedy - Actor One - Actor Two -  Actor Three"),
    ("Bhagam Bhag 2006 (HD) - Full Movie - Superhit Comedy Movie - Akshay Kumar - Govinda - Paresh Rawal",
     "Example Film (HD) - Full Movie - Comedy - Actor One - Actor Two - Actor Three"),
    ("Shemaroo Comedy", "Example Channel"),
]
SCRUB["crates/omadeck-core/tests/fixtures/hyprctl-clients.json"] = FILM + [("omarchy:Work", "shell")]
for p in [
          "crates/omadeck-core/tests/fixtures/playerctl-two.txt",
          "crates/omadeck-core/src/player.rs"]:
    SCRUB[p] = FILM

# Planning phrases left in the original bodies, rewritten wherever they occur.
MSG_SUBS = [
    ("use these from the next task.", "use these from the next commit."),
    ("(the daemon's real state lands in the next tasks)", "(the daemon's real state lands in the next commits)"),
    ("by the daemon (next PR tasks).", "by the daemon (in a later commit)."),
    ("edits the config as upstream.", "edits the config as before."),
]

def rewrap(msg):
    """Re-wrap body paragraphs (not lists) that an edit left with a line over 76."""
    import textwrap
    head, _, body = msg.partition("\n\n")
    paras = []
    for p in body.split("\n\n"):
        lines = p.split("\n")
        if any(len(l) > 76 for l in lines) and not any(l.lstrip().startswith(("- ", "* ")) for l in lines):
            p = textwrap.fill(" ".join(l.strip() for l in lines), 72, break_long_words=False, break_on_hyphens=False)
        paras.append(p)
    return head + ("\n\n" + "\n\n".join(paras) if body else "")

def M(subject, body=""):
    return subject + ("\n\n" + body.strip() if body.strip() else "")

def orig_msg(sha, edits=()):
    m = out("log", "-1", "--format=%B", sha)
    m = "\n".join(l for l in m.splitlines() if not l.startswith(("Co-Authored-By:", "Folded from")))
    # no "Tested on <my deck>" paragraphs: testing goes in the PR description
    m = "\n\n".join(p for p in m.split("\n\n") if not p.startswith("Tested"))
    for a, b in edits:
        assert a in m, (sha, a)
        m = m.replace(a, b)
    return m.strip()

# (branch, [(task shas, message)]); each group ends at the commit before the next
# group's first task (the last group ends at WORK).
GROUPS = [
 ("up/01-plus-support", [
  (["d6d16a8"], lambda: orig_msg("d6d16a8")),
  (["1332fc9"], lambda: orig_msg("1332fc9")),
  (["2b7012c"], lambda: orig_msg("2b7012c")),
 ]),
 ("up/02-dials-and-strip", [
  (["f60161f", "cf1912a"], lambda: orig_msg("f60161f") + "\n\n"
   "On Omarchy 4 the mic has no clamping command of its own, so its bare\n"
   "wpctl call gets -l 1.0 and stops at 100% like the output."),
  (["9f4acc9"], lambda: M("Page with a strip flick or a page_step action", orig_msg("9f4acc9").split("\n", 1)[1])),
  (["44e0664"], lambda: orig_msg("44e0664", [("Running it surfaced three fixes:", "Along the way:"),
   ("touched by the daemon.", "touched by the daemon. A brightness edited in config.toml while the daemon\nwas stopped wins at the next start.")])),
  (["5c56a4b", "5b07435", "006a48a", "a6dfb1a"], lambda: M("Draw background images on the touch strip and dials", orig_msg("a6dfb1a").split("\n", 1)[1])),
  (["a9dc7f0", "e5e61f2"], lambda: M("Pick strip and dial background images in the configurator",
   orig_msg("a9dc7f0").split("\n", 1)[1] + "\n\nThe README and changelog document both settings.")),
 ]),
 ("up/03-groundwork", [
  (["35b284e"], lambda: M("Run commands with a timeout; volume changes that can be waited for", orig_msg("35b284e").split("\n", 1)[1])),
  (["9afc26a"], lambda: M("Add probes, values and one panel builder", orig_msg("9afc26a", [("so Task 8 can send\npanels to the configurator", "so the daemon can send\npanels to the configurator")]).split("\n", 1)[1])),
  (["a01ed2c"], lambda: M("Keep the deck-owned actions in one list; address dial slots by direction", orig_msg("a01ed2c").split("\n", 1)[1])),
  (["17fcda5"], lambda: M("Split out the bar drawer; never stat a background image on paint", orig_msg("17fcda5", [("so\nPR 8 can draw key bars and PR 10 can tone each part", "so\nother callers can draw bars and pick each part's colours")]).split("\n", 1)[1])),
  (["bfe0f97"], lambda: M("Save the state file whole and atomically", orig_msg("bfe0f97").split("\n", 1)[1])),
  (["0c67a2b"], lambda: M("Replace the audio worker with a watcher thread", orig_msg("0c67a2b").split("\n", 1)[1])),
  (["f039e67"], lambda: M("Split the daemon: Deck, Runtime and Daemon, one input path, deadlines", orig_msg("f039e67").split("\n", 1)[1])),
  (["351cbfe"], lambda: M("Never block on the event socket; send panels to the configurator", orig_msg("351cbfe").split("\n", 1)[1])),
  (["9fa70d7"], lambda: M("Configurator: mirror the daemon instead of polling", orig_msg("9fa70d7").split("\n", 1)[1])),
  (["516e4f7"], lambda: M("Configurator: one action slot type and one Try-it-now button", orig_msg("516e4f7", [("so editors nest —\nPR 12's step editor will build a slot for a step and call\naction_editor again", "so editors nest:\na nested editor builds a slot and calls action_editor again")]).split("\n", 1)[1])),
  (["561f17c"], lambda: M("Keep the volume bar from snapping back during a fast spin", orig_msg("561f17c").split("\n", 1)[1])),
  (["297bb15"], lambda: M("Runtime: a config brightness edited while stopped still wins at start",
   "Runtime::start and a reload compare the config's brightness with the value\n"
   "the state file last saw, as the deck loop did before the split, and the\n"
   "state records the value it has seen. A config that fails to parse at start\n"
   "counts as no edit.\n\n"
   "Also: every volume change is acknowledged exactly once, read or not, so the\n"
   "in-flight total can't drift; and Disconnected is published once, from\n"
   "detach, not on every scan.")),
 ]),
 ("up/04-toggle-keys", [
  (["f3cb8c8"], lambda: M("Add command probes, toggle values and a non-blocking job", orig_msg("f3cb8c8", [("The pieces PR 8's watcher and daemon build on:", "The pieces toggle keys build on:"), ("fields added later (PR 9's\n  live) join it with serde(default)", "fields added later join it\n  with serde(default)")]).split("\n", 1)[1])),
  (["ccceb77"], lambda: M("Add toggle faces to the config, and the key status builder", orig_msg("ccceb77").split("\n", 1)[1])),
  (["1fa45a0"], lambda: M("Draw keys in their state: off, on, busy, pressed", orig_msg("1fa45a0").split("\n", 1)[1])),
  (["bc12ea4"], lambda: M("Watcher: run state commands off-thread; poll fast while a toggle settles", orig_msg("bc12ea4", [("PR 8's toggles run on", "toggle keys run on")]).split("\n", 1)[1])),
  (["7e17bdc"], lambda: M("Publish key states on the event socket", orig_msg("7e17bdc").split("\n", 1)[1])),
  (["0b29952"], lambda: M("Show keys in their state, flip pressed toggles, watch the page's probes", orig_msg("0b29952").split("\n", 1)[1])),
  (["9a565c6"], lambda: M("Add busy_while_running: tracked jobs with a live progress face", orig_msg("9a565c6").split("\n", 1)[1])),
  (["136e5d8"], lambda: M("Configurator: mirror key states and edit each face in its own tab", orig_msg("136e5d8").split("\n", 1)[1])),
  (["de8f5cb"], lambda: M("Configurator: a Toggle group makes any key a toggle", orig_msg("de8f5cb").split("\n", 1)[1])),
  (["a66a14f"], lambda: M("Add toggle presets: one click makes a key a working toggle", orig_msg("a66a14f", [("from the commands captured on the\nmachine (plans/pr8-toggle-keys.md, \"Captured\")", "from commands checked on\nOmarchy")]).split("\n", 1)[1])),
  (["05ff183"], lambda: M("Use toggle keys in the starter layout; document them", orig_msg("05ff183").split("\n", 1)[1])),
 ]),
 ("up/05-live-keys", [
  (["0e79a38"], lambda: M("Add clock formatting and CPU, memory and battery readers",
   "live.rs formats clocks with jiff's strftime and works out when the next\n"
   "tick is due (on the minute or second, at most 5 s away). sysinfo.rs reads\n"
   "/proc/stat, /proc/meminfo and the power_supply class. jiff is already in\n"
   "the lock through env_logger; its tz-system feature adds one locked line.")),
  (["ee25523"], lambda: M("Add the live field to keys and dials, with its probes and values",
   "live on [[key]] and [[dial]] names what a key or a dial's panel shows: a\n"
   "clock, CPU, memory, battery, the Hyprland workspace or a command's output.\n"
   "Each source is a Probe; values travel as Text, Percent or Battery, and\n"
   "KeyStatus and PanelStatus carry what is shown. The daemon only gets\n"
   "placeholder arms here.")),
  (["1939e11"], lambda: M("Draw live values on keys and strip panels",
   "render_body draws a live value on both render paths (a busy face wins\n"
   "over it); text blocks break on newlines; the $A1 layout is $B1 without\n"
   "the bar; each source has a glyph.")),
  (["6c04ff8"], lambda: M("Watcher: read clocks, meters and command output",
   "Clock, CPU, memory and battery are read inline, the clock scheduled on\n"
   "the minute or second; a command runs on the runner threads with a 10 s\n"
   "timeout and keeps its last value on failure.")),
  (["c83368e"], lambda: M("Listen to Hyprland's event socket",
   "hypr.rs: a thread reads .socket2.sock, asks .socket.sock for the active\n"
   "workspace and window, and sends only changes. The next commit wires it in.")),
  (["ca96f35"], lambda: M("Wire live values and the Hyprland listener into the daemon",
   "The daemon watches, publishes and keeps live values (the workspace\n"
   "always), re-reads a live command key after a press, and applies\n"
   "Hyprland events after the watcher's values.")),
  (["354c794"], lambda: M("Configurator: Live value and Panel value groups",
   "A Shows choice per key and dial, a clock format row with a live example,\n"
   "a command row saved on Enter or focus-out, and its interval. The On and\n"
   "Busy tabs keep the mirrored live value in the preview.")),
  (["4e557a0"], lambda: M("Document live keys",
   "Commented clock, CPU and command keys and a battery dial in\n"
   "examples/config.toml, a README feature line, the roadmap item ticked,\n"
   "and the changelog.")),
 ]),
 ("up/06-app-volume", [
  (["eed0109"], lambda: M("Read playback streams from pactl",
   "pactl -f json list sink-inputs, where ids and pids may be strings or\nnumbers.")),
  (["30fa8f5"], lambda: M("Match playback streams to an app or the focused window", "Also the app's icon names and the wpctl commands that change a stream.")),
  (["055d250"], lambda: M("Add the app_volume action and its level source",
   "Outside the daemon (omadeckd press, the configurator's Try it now) it\nruns directly.")),
  (["7cad9d1"], lambda: M("Add the app probe, its value and what an app panel shows")),
  (["d53d81d"], lambda: M("Tone each text by the background under it; draw the app panel",
   "Each part of a panel (title, value, bar) takes a light or dark tone from\n"
   "the background image under its own rectangle, so text stays readable on\n"
   "an image that is dark in one place and light in another.")),
  (["eda46c1"], lambda: M("Watcher: read app streams, follow focus and change app volumes")),
  (["8d2aaf2"], lambda: M("Daemon: route app volume through the watcher", "The watcher gets the focused window; app values survive a reload.")),
  (["e8f8354"], lambda: M("Configurator: app volume rows, an app picker and dial presets")),
  (["93ad767"], lambda: M("Document per-app volume", "README, example config and changelog.")),
 ]),
 ("up/07-ctl-and-multi-actions", [
  (["1d914ac"], lambda: M("Add control commands and replies, and a request client")),
  (["cbb12aa"], lambda: M("Hub: carry replies back to the client that asked; make the socket 0600",
   "A reply is never dropped, even when that client's event queue is full.")),
  (["224df6c"], lambda: M("Daemon: input methods that return results; a ctl press flashes the key")),
  (["9017d06"], lambda: M("Daemon: answer ctl commands; a reload returns its error", "press, dial, run, page, reload and state.")),
  (["9dd70d8"], lambda: M("omadeckd: page, brightness, reload and state commands", "press and dial go through the running daemon too.")),
  (["4ad077f"], lambda: M("Configurator: Try it now goes through the daemon; document ctl",
   "A key's own ▶ presses the key. README, site and changelog.")),
  (["cfb26c8"], lambda: M("Add multi and delay actions",
   "A multi runs its steps in order; a delay step waits between them. Step\n"
   "kinds, summaries and the action walker's multi arm.")),
  (["b4198e2"], lambda: M("Run a multi without a deck loop; run() refuses a deck-owned multi")),
  (["9273991"], lambda: M("Daemon: a sequencer that times steps by deadlines, one run per input")),
  (["d479f9b"], lambda: M("Daemon: run multis from the loop",
   "Steps run as Source::Step; a running multi ignores its key until it ends;\n"
   "omadeckd press and dial run multis too.")),
  (["8d0976b"], lambda: M("Configurator: kind lists per slot, the Delay editor and step slots")),
  (["96696c8"], lambda: M("Configurator: edit a multi's steps as a list; document multi actions",
   "Add, move, remove and edit a step; Try it now runs the whole multi.\n"
   "README, example config and changelog.")),
 ]),
 ("up/08-media-dial", [
  (["9fd5fc8"], lambda: M("Add the media action: play/pause, next, previous, stop and seek")),
  (["3823516"], lambda: M("Read and pick a media player through playerctl", "Local cover art, the same-track test, and the media commands per audio\nbackend.")),
  (["0307a8c"], lambda: M("Add the player probe and value; play/pause keys follow the player")),
  (["3a385ca"], lambda: M("Draw the media panel: title, cover or glyph, artist, time and progress")),
  (["5869950"], lambda: M("Watcher: read the player once a second; run media commands in order", "Covers are decoded here, and only real changes are sent.")),
  (["11fbbbc"], lambda: M("Daemon: dispatch media commands; repaint a playing panel", "The shown track moves at once and holds until a read settles it; a\nplaying panel repaints every second; a reload keeps the track.")),
  (["b13d5d8"], lambda: M("Configurator: media actions and presets; document the media dial", "Media action rows, Media dial and Play/Pause presets, and a preview that\nmoves the track. README, example config and changelog.")),
 ]),
 ("up/09-page-navigation", [
  (["b661698"], lambda: M("Step pages from nested folders, starting at the page they hang off",
   "Config::opener and page_path find the page a folder opens from;\nNav::nearest starts a page step there.")),
  (["5928c98"], lambda: M("Back acts on release; hold it for half a second to go Home",
   "Held keys stay drawn pressed until they are released.")),
  (["fc5dbdf"], lambda: M("Configurator: double-click to open folders; folder paths in the switcher",
   "Double-click a folder key to open it or a Back key to go up. README and\nchangelog.")),
  (["e97842e"], lambda: M("Let pages follow apps: apps on [[page]], class globs, open windows",
   "The first non-folder page that lists the focused window's class wins.")),
  (["c580b89"], lambda: M("Daemon: show a page while its app is focused, and go back after",
   "Focus settles for 200 ms; the configurator's own window is ignored; a\n"
   "page picked by hand stays; a reload decides again.")),
  (["0042e25"], lambda: M("Configurator: a Follow apps dialog in the page bar; document app pages")),
 ]),
 ("up/10-strip-long-press", [
  (["f7cef95"], lambda: M("Run a dial's long_press when its strip panel is held",
   "Without one, a hold runs the press. A long press shares the press's run\nand its failure report.")),
  (["1bf8c07"], lambda: M("Configurator: a Long press row on decks with a strip; document it")),
 ]),
]

# file -> branch-relative new-commit index (0-based) to place it at instead
OVERRIDE = {
    "1:crates/omadeck-core/tests/fixtures/test-font.ttf": 3,
    "1:crates/omadeck-core/tests/fixtures/test-font-OFL.txt": 3,
    "2:crates/omadeckd/src/main.rs": 11,
    "2:crates/omadeckd/src/watch.rs": 11,
    "3:crates/omadeck-core/src/probe.rs": 6,
    "3:crates/omadeckd/src/hypr_worker.rs": 6,
    "3:crates/omadeckd/Cargo.toml": 6,
    "3:crates/omadeck-gui/src/editor.rs": 10,
    "6:crates/omadeck-core/src/ipc.rs": 1,
    "3:crates/omadeck-core/src/action.rs": 6,
    "7:crates/omadeck-core/src/player.rs": 5,
    "7:crates/omadeck-core/tests/fixtures/playerctl-two.txt": 5,
    "7:crates/omadeck-core/src/action.rs": 5,
    "7:crates/omadeckd/src/hub.rs": 5,
    "7:crates/omadeckd/src/hypr_worker.rs": 5,
}
FORMATTED = {}

def rustfmt(src):
    """The repo's rustfmt settings, on one file's text (a fix's code is formatted like the rest)."""
    if src not in FORMATTED:
        r = subprocess.run(["rustfmt", "--edition", "2024", "--config", "max_width=120,use_small_heuristics=Max",
                            "--emit", "stdout"], input=src.encode(), capture_output=True)
        if r.returncode != 0:
            sys.exit(f"rustfmt failed: {r.stderr.decode()[:500]}")
        FORMATTED[src] = r.stdout.decode()
    return FORMATTED[src]

CLEANED = {}
SKIPPED = []

def cleaned_blob(blob):
    if blob not in CLEANED:
        data = git("cat-file", "blob", blob).decode()
        new = clean(data)
        CLEANED[blob] = out("hash-object", "-w", "--stdin", input=new.encode()) if new != data else blob
    return CLEANED[blob]

def changed(sha):
    return [p for p in out("diff-tree", "--no-commit-id", "--name-only", "-r", "--no-renames", sha + "^", sha).splitlines() if p]

def main():
    series = out("rev-list", "--reverse", f"{BASE}..{WORK}").splitlines()
    short = {s[:7]: s for s in series}
    def full(s):
        return short.get(s[:7]) or sys.exit(f"{s} not in series")
    pos = {s: i for i, s in enumerate(series)}
    firsts = [pos[full(g[1][0][0][0])] for g in GROUPS]
    ends = [f - 1 for f in firsts[1:]] + [len(series) - 1]
    assert firsts[0] == 0
    parent = out("rev-parse", BASE)
    ident = {}
    tmpidx = tempfile.mktemp(prefix="upidx-", dir=os.environ.get("TMPDIR"))
    E = {"GIT_INDEX_FILE": tmpidx}
    report = []
    for gi, (branch, commits) in enumerate(GROUPS):
        lo, hi = firsts[gi], ends[gi]
        tasks = {full(t) for c in commits for t in c[0]}
        allc = series[lo:hi + 1]
        fixes = [c for c in allc if c not in tasks]
        newidx = {full(t): k for k, c in enumerate(commits) for t in c[0]}
        last_task = {k: max(pos[full(t)] for t in c[0]) for k, c in enumerate(commits)}
        for k in range(1, len(commits)):
            assert last_task[k] > last_task[k - 1], (branch, k)
        touched_by = {}
        for t in tasks:
            for f in changed(t):
                touched_by[f] = max(touched_by.get(f, -1), newidx[t])
        fix_files = {}
        for c in fixes:
            for f in changed(c):
                fix_files[f] = max(fix_files.get(f, -1), pos[c])
        target = {}
        for f, last_fix in fix_files.items():
            if f in touched_by:
                target[f] = touched_by[f]
            else:
                before = [k for k in range(len(commits)) if last_task[k] < last_fix]
                target[f] = before[-1] if before else 0
            if f"{gi}:{f}" in OVERRIDE:
                target[f] = OVERRIDE[f"{gi}:{f}"]
        final = series[hi]
        for k, (shas, msgf) in enumerate(commits):
            src = series[last_task[k]]
            git("read-tree", src, env=E)
            for f, tk in target.items():
                if tk > k:
                    continue
                ent = out("ls-tree", final, "--", f)
                if ent:
                    meta, _ = ent.split("\t", 1)
                    mode, _, blob = meta.split()
                    git("update-index", "--add", "--cacheinfo", f"{mode},{blob},{f}", env=E)
                else:
                    git("update-index", "--force-remove", "--", f, env=E)
            for lo_g, hi_g, f, hunks, gone in FIXES:
                if not lo_g <= gi <= hi_g:
                    continue
                ent = out("ls-files", "-s", "--", f, env=E)
                if not ent:
                    continue
                mode, blob = ent.split()[0], ent.split()[1]
                data = git("cat-file", "blob", blob).decode()
                # All or nothing: a fix whose rewrites don't all match this version is
                # skipped here (reported), never half applied.
                if not all(a in data or b in data for a, b in hunks):
                    SKIPPED.append((branch, k, f, hunks[0][0][:50]))
                    if any(g in data for g in gone):
                        sys.exit(f"{branch} commit {k}: {f} has the bug but the fix didn't match")
                    continue
                new = data
                for a, b in hunks:
                    new = new.replace(a, b)
                left = [g for g in gone if g in new]
                if left:
                    sys.exit(f"{branch} commit {k}: {f} still has {left!r}")
                if new != data:
                    new = rustfmt(new)
                    nb = out("hash-object", "-w", "--stdin", input=new.encode())
                    git("update-index", "--cacheinfo", f"{mode},{nb},{f}", env=E)
            for f, reps in SCRUB.items():
                ent = out("ls-files", "-s", "--", f, env=E)
                if not ent:
                    continue
                mode, blob = ent.split()[0], ent.split()[1]
                data = git("cat-file", "blob", blob).decode()
                new = data
                for a, b in reps:
                    new = new.replace(a, b)
                if new != data:
                    nb = out("hash-object", "-w", "--stdin", input=new.encode())
                    git("update-index", "--cacheinfo", f"{mode},{nb},{f}", env=E)
            for ent in out("ls-files", "-s", env=E).splitlines():
                meta, f = ent.split("\t", 1)
                if not f.endswith(".rs") or "/captured/" in f:
                    continue
                mode, blob = meta.split()[0], meta.split()[1]
                nb = cleaned_blob(blob)
                if nb != blob:
                    git("update-index", "--cacheinfo", f"{mode},{nb},{f}", env=E)
            tree = out("write-tree", env=E)
            first = full(shas[0])
            a = out("log", "-1", "--format=%an%x00%ae%x00%ad", "--date=raw", first).split("\x00")
            env = {"GIT_AUTHOR_NAME": a[0], "GIT_AUTHOR_EMAIL": a[1], "GIT_AUTHOR_DATE": a[2]}
            body = msgf().rstrip()
            for x, y in MSG_SUBS:
                body = body.replace(x, y)
            msg = rewrap(body) + TRAILER
            parent = out("commit-tree", tree, "-p", parent, "-F", "-", input=msg.encode(), env=env)
            report.append(f"{branch} {k} {parent[:7]} {msg.splitlines()[0]}")
        git("branch", "-f", branch, parent)
        placed = {f: commits[t][0][0] for f, t in sorted(target.items())}
        report.append(f"  placements: {json.dumps(placed)}")
    os.unlink(tmpidx)
    print("\n".join(report))
    for sk in SKIPPED:
        print("skipped", *sk)

main()
