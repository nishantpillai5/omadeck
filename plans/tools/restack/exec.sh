#!/bin/zsh
# Run by `git rebase -x` after each restacked commit: give it the CHANGELOG it should have
# on upstream's release (amending if needed), then type-check; a failure stops the rebase
# at this commit.
export CARGO_HOME=/tmp/claude-502/cargo-home CARGO_TARGET_DIR=/tmp/claude-502/target-regroup
export PKG_CONFIG=/private/tmp/claude-502/-Users-nishant-work-personal-omadeck/e84142d9-c00d-49ba-9e7f-c7a88826475e/scratchpad/review3/gui-a/bin/pkg-config
R=/private/tmp/claude-502/-Users-nishant-work-personal-omadeck/e84142d9-c00d-49ba-9e7f-c7a88826475e/scratchpad/regroup/restack
subj=$(git log -1 --format=%s HEAD)
orig=$(git log --format='%H%x09%s' fb9052f..up/pre-v0.2 | awk -F'\t' -v s="$subj" '$2 == s { print $1 }')
if [[ -z $orig ]]; then echo "no original commit for: $subj"; exit 1; fi
python3 $R/changelog.py $orig upstream/master > CHANGELOG.md.new && /bin/mv -f CHANGELOG.md.new CHANGELOG.md
if ! git diff --quiet -- CHANGELOG.md; then
  git add CHANGELOG.md && git commit -q --amend --no-edit
fi
out=$(cargo check --workspace --all-targets --offline 2>&1)
st=$?
if [[ $st != 0 ]]; then
  print -r -- "$out" | grep -E '^error|^\s+--> ' | head -40
  echo "CHECK FAILED at: $subj"
fi
exit $st
