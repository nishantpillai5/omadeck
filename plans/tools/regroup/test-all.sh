#!/bin/zsh
# Run the core and daemon tests at every commit of the up/* series. The hub's socket
# tests can't bind in the sandbox, so their failures are ignored.
export CARGO_HOME=/tmp/claude-502/cargo-home CARGO_TARGET_DIR=/tmp/claude-502/target-regroup
cd /Users/nishant/work/personal/omadeck/.claude/worktrees/regroup || exit 1
for c in $(git rev-list --reverse fb9052f..up/15-strip-long-press); do
  git checkout -q --detach $c || exit 1
  t=$(cargo test -p omadeck-core -p omadeckd --offline --no-fail-fast 2>&1)
  fails=$(print -r -- "$t" | grep -E '^test .* FAILED$' | grep -v -E 'new_client_gets_the_replay|a_client_that_never_reads_does_not_block_publish|command_reply_over_socket')
  built=$(print -r -- "$t" | grep -c -E '^error\[|could not compile')
  if [[ -n $fails || $built != 0 ]]; then
    echo "FAIL ${c:0:7} $(git log -1 --format=%s $c | cut -c1-70)"
    print -r -- "$fails" | sed 's/^/     /'
    print -r -- "$t" | grep -E '^error' | head -5 | sed 's/^/     /'
  else
    echo "ok   ${c:0:7} $(git log -1 --format=%s $c | cut -c1-70)"
  fi
done
echo DONE
