"""Code the fold left in the wrong commit, moved by patch (build.py). Each move: the group
(0-based), the new-commit range in it (inclusive), a patch in patches/, and whether it is
applied in reverse (taking the change out of those commits, so it first appears after
them) or forward (bringing it into them, earlier than the fold put it)."""
import os

D = os.path.join(os.path.dirname(os.path.abspath(__file__)), "patches")

PATCHES = [
    # Toggle keys: the Hyprland worker gets its own commit (11) instead of riding in
    # busy_while_running (6) and the commits after it.
    (6, 6, 10, f"{D}/pr4-worker-daemon.patch", True),
    (6, 6, 10, f"{D}/pr4-worker-main.patch", True),
    (6, 6, 10, f"{D}/pr4-worker-action.patch", True),
    (6, 10, 10, f"{D}/pr4-worker-changelog.patch", True),
    # Media dial: the test daemon gets its own commit (7) instead of riding in the media
    # daemon commit (5) and the configurator commit after it.
    (11, 5, 6, f"{D}/pr8-harness-daemon.patch", True),
    (11, 5, 6, f"{D}/pr8-harness-hub.patch", True),
    (11, 5, 6, f"{D}/pr8-harness-watch.patch", True),
    (11, 5, 6, f"{D}/pr8-harness-hypr_worker.patch", True),
]

# Not for upstream: only the private capture tests read it (it goes to up/captures).
DROP_PATHS = ["crates/omadeck-core/tests/fixtures/activewindow-empty.json"]
