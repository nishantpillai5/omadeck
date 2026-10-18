#!/bin/zsh
# Type-check every commit of the up/* series (GUI against the stub pkg-config).
# Usage: check-all.sh [first-sha]   (resume from a commit)
export CARGO_HOME=/tmp/claude-502/cargo-home CARGO_TARGET_DIR=/tmp/claude-502/target-regroup
export PKG_CONFIG=/private/tmp/claude-502/-Users-nishant-work-personal-omadeck/e84142d9-c00d-49ba-9e7f-c7a88826475e/scratchpad/review3/gui-a/bin/pkg-config
cd /Users/nishant/work/personal/omadeck/.claude/worktrees/regroup || exit 1
start=${1:-}
skip=${start:+1}
for c in $(git rev-list --reverse fb9052f..up/15-strip-long-press); do
  if [[ -n $skip ]]; then [[ $c == $start* ]] && skip= || continue; fi
  git checkout -q --detach $c || exit 1
  if out=$(cargo check --workspace --all-targets --offline 2>&1); then
    echo "ok   ${c:0:7} $(git log -1 --format=%s $c | cut -c1-80)"
  else
    echo "FAIL ${c:0:7} $(git log -1 --format=%s $c | cut -c1-80)"
    print -r -- "$out" | grep -E '^(error|warning: unused)|^\s+--> ' | head -30 | sed 's/^/     /'
  fi
done
echo DONE
