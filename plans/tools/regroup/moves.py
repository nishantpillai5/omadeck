"""Code the fold left in the wrong commit, moved by patch (build.py). Each move: the group
(0-based), the new-commit range in it (inclusive), a patch in patches/, and whether it is
applied in reverse (taking the change out of those commits, so it first appears after
them) or forward (bringing it into them, earlier than the fold put it)."""
import os

D = os.path.join(os.path.dirname(os.path.abspath(__file__)), "patches")

PATCHES = [
    # PR 2: commit 3 is the daemon's own fixes; the background images' two daemon hunks
    # stay with the background images (commit 4).
    (1, 3, 3, f"{D}/pr2-backgrounds-daemon.patch", True),
    # PR 4: the Hyprland worker gets its own commit (11) instead of riding in
    # busy_while_running (6) and the commits after it.
    (3, 6, 10, f"{D}/pr4-worker-daemon.patch", True),
    (3, 6, 10, f"{D}/pr4-worker-main.patch", True),
    (3, 6, 10, f"{D}/pr4-worker-action.patch", True),
    (3, 10, 10, f"{D}/pr4-worker-changelog.patch", True),
    # PR 8: the test daemon gets its own commit (7) instead of riding in the media
    # daemon commit (5) and the configurator commit after it.
    (7, 5, 6, f"{D}/pr8-harness-daemon.patch", True),
    (7, 5, 6, f"{D}/pr8-harness-hub.patch", True),
    (7, 5, 6, f"{D}/pr8-harness-watch.patch", True),
    (7, 5, 6, f"{D}/pr8-harness-hypr_worker.patch", True),
    # PR 9: the deck frame fix belongs to the folders' configurator commit (2), not the
    # app pages' one (5).
    (8, 2, 2, f"{D}/pr9-deck-frame.patch", False),
]

# Not for upstream: only the private capture tests read it (it goes to up/captures).
DROP_PATHS = ["crates/omadeck-core/tests/fixtures/activewindow-empty.json"]
