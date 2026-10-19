#!/bin/zsh
# Drive the restack: a CHANGELOG conflict is resolved by regenerating the file from the
# commit being applied; anything else (a code conflict, a failed check) stops here.
R=/private/tmp/claude-502/-Users-nishant-work-personal-omadeck/e84142d9-c00d-49ba-9e7f-c7a88826475e/scratchpad/regroup/restack
cd /Users/nishant/work/personal/omadeck/.claude/worktrees/regroup || exit 1
while true; do
  gd=$(git rev-parse --git-dir)
  if [[ ! -d $gd/rebase-merge && ! -d $gd/rebase-apply ]]; then echo "REBASE DONE"; exit 0; fi
  conflicted=(${(f)"$(git diff --name-only --diff-filter=U)"})
  if [[ ${#conflicted} -gt 0 ]]; then
    others=(${conflicted:#CHANGELOG.md})
    if [[ ${#others} -gt 0 ]]; then
      left=$(python3 $R/autoresolve.py ${others[@]})
      for f in ${others[@]}; do
        [[ " $left " == *" $f "* || -n ${(M)${(f)left}:#$f} ]] || git add $f
      done
      others=(${(f)left})
    fi
    if [[ ${#others} -gt 0 ]]; then
      echo "CODE CONFLICT in: ${others[*]}"
      echo "applying: $(git log -1 --format='%h %s' REBASE_HEAD)"
      exit 2
    fi
    python3 $R/changelog.py REBASE_HEAD upstream/master > CHANGELOG.md.new && /bin/mv -f CHANGELOG.md.new CHANGELOG.md
    git add CHANGELOG.md
  fi
  out=$(GIT_EDITOR=true git -c rerere.enabled=true rebase --continue 2>&1)
  st=$?
  if [[ $st != 0 ]]; then
    if print -r -- "$out" | grep -q 'CHECK FAILED'; then
      print -r -- "$out" | grep -E '^error|-->|CHECK FAILED' | head -40
      exit 3
    fi
    if ! print -r -- "$out" | grep -q 'CONFLICT'; then
      print -r -- "$out" | tail -20
      exit 4
    fi
  fi
done
