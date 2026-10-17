#!/bin/zsh
# fmt, clippy -D warnings and the core + daemon tests on each up/* tip.
# The GUI is type-checked by clippy (stub pkg-config); it can't link here. The hub's
# socket tests can't bind in the sandbox, so they are listed, not counted as failures.
export CARGO_HOME=/tmp/claude-502/cargo-home CARGO_TARGET_DIR=/tmp/claude-502/target-regroup
export PKG_CONFIG=/private/tmp/claude-502/-Users-nishant-work-personal-omadeck/e84142d9-c00d-49ba-9e7f-c7a88826475e/scratchpad/review3/gui-a/bin/pkg-config
cd /Users/nishant/work/personal/omadeck/.claude/worktrees/regroup || exit 1
for b in $(git for-each-ref --format='%(refname:short)' 'refs/heads/up/[0-9]*'); do
  git checkout -q --detach $b || exit 1
  r=""
  cargo fmt --all --check >/dev/null 2>&1 || r+=" FMT"
  cargo clippy --workspace --all-targets --offline -- -D warnings >/dev/null 2>&1 || r+=" CLIPPY"
  t=$(cargo test -p omadeck-core -p omadeckd --offline 2>&1)
  fails=$(print -r -- "$t" | grep -E '^test .* FAILED$' | grep -v -E 'new_client_gets_the_replay|a_client_that_never_reads_does_not_block_publish|command_reply_over_socket' )
  print -r -- "$t" | grep -q '^error\[' && r+=" TEST-BUILD"
  [[ -n $fails ]] && r+=" TESTS"
  passed=$(print -r -- "$t" | grep -oE '[0-9]+ passed' | awk '{s+=$1} END {print s}')
  echo "${b}: ${r:- ok} (passed $passed)"
  [[ -n $fails ]] && print -r -- "$fails" | sed 's/^/    /'
done
echo DONE
