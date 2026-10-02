#!/usr/bin/env bash
# Branch divergence guard for the long-lived fork branches (dev, experimental).
#
# WHY: the dev->experimental backport of 2026-10-02 spanned 288 files because
# the pair drifted for weeks. The worst slice was invisible to plain conflict
# counts: `query.rs` was split into `runtimes/query/` on one side while
# concurrently edited on the other, so git reported no conflict until an E0761
# module collision mid-merge, and the hand-composed resolution silently dropped
# a brace and a re-export target. Two cheap signals catch that weeks earlier:
#
#   1. would a merge conflict TODAY (git merge-tree, no working tree needed)
#   2. dual-touch files: both sides edited the same path since the merge base
#      — files git would merge cleanly but semantically fork, the dangerous set
#
# Exit 0 = healthy (or branches absent, nothing to check). Exit 1 = diverged:
# merge would conflict, or dual-touch count reached the threshold.
#
# Body for the divergence issue is written to $2. The run summary gets the same
# report via $GITHUB_STEP_SUMMARY when set.
#
# Local use mirrors what CI does:
#   git merge-tree --write-tree dev experimental    # exit 1 = would conflict
#   git merge-tree --write-tree --name-only dev experimental
#     # line 1: result tree OID, then the conflicted file names
#   base=$(git merge-base dev experimental)
#   comm -12 <(git diff --name-only "$base" dev | sort) \
#            <(git diff --name-only "$base" experimental | sort)
#
# Usage: check-branch-divergence.sh <ref-a> <ref-b> <body-file>
# Env:   DUAL_TOUCH_MIN  dual-touch count that counts as diverged (default 20)

set -euo pipefail

REF_A="${1:?usage: check-branch-divergence.sh <ref-a> <ref-b> <body-file>}"
REF_B="${2:?missing <ref-b>}"
BODY_FILE="${3:?missing <body-file>}"
DUAL_TOUCH_MIN="${DUAL_TOUCH_MIN:-20}"

# A branch that has never been pushed has nothing to check; that is healthy
# (nothing diverged), not an error.
for ref in "$REF_A" "$REF_B"; do
  if ! git rev-parse --verify --quiet "$ref" >/dev/null; then
    echo "$ref not present; nothing to check." >"$BODY_FILE"
    exit 0
  fi
done

base=$(git merge-base "$REF_A" "$REF_B")

# --- signal 1: what a merge would conflict on, right now, without touching
# the working tree. merge-tree exits 1 when the merge would not be clean.
merge_out=$(git merge-tree --write-tree --name-only "$REF_A" "$REF_B" 2>/dev/null) && merge_rc=0 || merge_rc=$?
if [ "$merge_rc" -le 1 ]; then
  # exit 0: single OID line. exit 1: OID, blank, then conflicted paths.
  conflicted=$(printf '%s\n' "$merge_out" | sed -n '3,$p')
else
  echo "git merge-tree failed (rc=$merge_rc); git too old or refs unusable." >&2
  exit "$merge_rc"
fi

# --- signal 2: dual-touch files. Both sides moved since the merge base is the
# semantic-fork predictor; git's clean merge there is a false negative.
changed_a=$(mktemp); changed_b=$(mktemp); dual=$(mktemp)
trap 'rm -f "$changed_a" "$changed_b" "$dual"' EXIT
git diff --name-only "$base" "$REF_A" | sort >"$changed_a"
git diff --name-only "$base" "$REF_B" | sort >"$changed_b"

# Keep only dual-touches whose two tips still differ; identical end states are
# already reconciled (one side carried the other's change).
comm -12 "$changed_a" "$changed_b" | while IFS= read -r f; do
  if ! git diff --quiet "$REF_A" "$REF_B" -- "$f"; then
    printf '%s\n' "$f"
  fi
done >"$dual"

conflict_count=$(printf '%s' "${conflicted:-}" | grep -c . || true)
dual_count=$(grep -c . "$dual" || true)
diverged=false
if [ "$conflict_count" -gt 0 ] || [ "$dual_count" -ge "$DUAL_TOUCH_MIN" ]; then
  diverged=true
fi

{
  echo "Divergence between \`$REF_A\` and \`$REF_B\` at merge base $(git log -1 --format='%h %s' "$base")."
  echo
  echo "- merge would conflict on: **$conflict_count** file(s)"
  echo "- dual-touch files (both sides edited since base, tips differ): **$dual_count** (threshold $DUAL_TOUCH_MIN)"
  if [ "$conflict_count" -gt 0 ]; then
    echo
    echo "### Conflicting files"
    echo '```'
    cat <<<"$conflicted"
    echo '```'
  fi
  if [ "$dual_count" -gt 0 ]; then
    echo
    echo "### Dual-touch files"
    echo '```'
    cat "$dual"
    echo '```'
  fi
  echo
  echo "Backport one side into the other while the lists are short:"
  echo '```'
  echo "git checkout $REF_B && git merge $REF_A"
  echo '```'
} >"$BODY_FILE"

cat "$BODY_FILE"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat "$BODY_FILE" >>"$GITHUB_STEP_SUMMARY"
fi

if [ "$diverged" = true ]; then
  echo "::warning::$REF_A/$REF_B diverged: $conflict_count conflicts, $dual_count dual-touch"
  exit 1
fi
