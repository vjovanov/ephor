#!/usr/bin/env bash
# The `status` verb of ephor's gate (§FS-006-project-interface.6): what the
# gate is doing, per repository of the forest.
#
# ephor's forest is one repository, so the breakdown has one row — but it is
# still a breakdown, because the shape is the seam's and a single number could
# not say which repository went red for a project whose forest is a tree
# (§AR-004-forest.1).
#
# In:  EPHOR_REPO, EPHOR_NUMBER, EPHOR_BRANCH name the matter; with none of
#      them, the branch this checkout is standing on.
# Out: exit 0 and the gate in $EPHOR_ANSWER; non-zero where the forge could not
#      be asked. A gate nobody could ask is not a green one.
set -uo pipefail

# shellcheck source=scripts/gate-lib.sh
. "$(dirname "$0")/gate-lib.sh"

repo=$(gate_repo)
if [ -z "$repo" ]; then
  gate_unreachable "this checkout" "nothing says which repository, and it has no GitHub remote"
  exit 1
fi

ref=$(gate_ref "$repo")
subject=$(gate_subject "$repo" "$ref")
if [ -z "$ref" ]; then
  gate_unreachable "$subject" "nothing says which branch or commit"
  exit 1
fi

if ! runs=$(gate_runs "$repo" "$ref"); then
  gate_unreachable "$subject" "the forge did not answer"
  exit 1
fi

counts=$(printf '%s' "$runs" | gate_counts)
passed=$(jq -r .passed <<<"$counts")
failed=$(jq -r .failed <<<"$counts")
running=$(jq -r .running <<<"$counts")

# What the forge refuses regardless of counts, where there is a pull request to
# refuse. On a branch with none, nothing is being held up: there is no merge to
# block, and inventing a blocker would put a red line under a green tree.
blockers='[]'
number=$(gate_clean "${EPHOR_NUMBER:-}")
if [ -n "$number" ]; then
  view=$(gh pr view "$number" --repo "$repo" \
           --json isDraft,mergeable,mergeStateStatus,reviewDecision 2>/dev/null)
  if [ -n "$view" ]; then
    blockers=$(jq -c '[
      (select(.isDraft) | "the pull request is still a draft"),
      (select(.mergeable == "CONFLICTING") | "the branch conflicts with its base"),
      (select(.reviewDecision == "CHANGES_REQUESTED") | "a reviewer asked for changes"),
      (select(.reviewDecision == "REVIEW_REQUIRED") | "a review is required and has not been given"),
      (select(.mergeStateStatus == "BLOCKED") | "the branch protection rules are not satisfied")
    ]' <<<"$view")
  fi
fi
blocked=$(jq -r 'length > 0' <<<"$blockers")

# Where a person would look, which is not the same page in both cases: a pull
# request has a checks tab of its own, and a branch has only the run list.
if [ -n "$number" ]; then
  url="https://github.com/$repo/pull/$number/checks"
else
  url="https://github.com/$repo/actions?query=branch%3A$ref"
fi
jq -n --arg repo "$repo" --arg url "$url" --argjson counts "$counts" \
      --argjson blockers "$blockers" --argjson blocked "$blocked" \
      --arg summary "$passed passed, $failed failed, $running running" '
  {v: 1,
   summary: $summary,
   url: $url,
   gate: {repos: [{repo: $repo,
                   passed: $counts.passed,
                   failed: $counts.failed,
                   running: $counts.running,
                   url: $url}],
          blocked: $blocked,
          blockers: $blockers}}' | gate_answer

echo "$subject: $passed passed, $failed failed, $running running"
jq -r '.[]' <<<"$blockers" | while IFS= read -r blocker; do
  [ -n "$blocker" ] && echo "  blocked: $blocker"
done
exit 0
