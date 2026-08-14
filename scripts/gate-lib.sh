#!/usr/bin/env bash
# Shared by ephor's three gate verbs (§FS-006-project-interface.6). Sourced,
# never run.
#
# ephor's gate is GitHub Actions, and ephor is worked by pushing to a branch as
# often as by opening a pull request — so what these verbs ask about is a
# *commit*, not a pull request. Check runs answer for both: a pull request's
# head sha and a bare branch tip are the same lookup, so the same three scripts
# serve a matter the feed reported and a checkout somebody is standing in.
#
# The forge could answer this on its own — ephor ships a GitHub provider that
# reports a gate, and where it does, nothing above the seam can tell the
# difference (§FS-006-project-interface.6). It only reports one for a matter it
# has cached, which is a pull request. Binding the verbs here is what makes the
# question answerable from the checkout alone, on a branch with no pull request
# on it, which is how this project is actually developed.

# A summons value that never resolved is worse than an empty one: it would be
# handed to the forge as text. `ephor` filters these out of its own arguments
# for the same reason.
gate_clean() {
  local value=${1:-}
  case "$value" in *'{'*) value= ;; esac
  printf '%s' "$value"
}

# The repository the gate lives in: what the summons named, else whatever this
# checkout pushes to.
gate_repo() {
  local repo
  repo=$(gate_clean "${EPHOR_REPO:-}")
  if [ -z "$repo" ]; then
    repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null) || repo=
  fi
  printf '%s' "$repo"
}

# The ref whose gate is being asked about, in the order the answer gets more
# specific: the head sha of the pull request the summons named, else the branch
# it named, else the branch this checkout is standing on.
gate_ref() {
  local repo=$1 number branch
  number=$(gate_clean "${EPHOR_NUMBER:-}")
  if [ -n "$number" ] && [ -n "$repo" ]; then
    local sha
    sha=$(gh pr view "$number" --repo "$repo" --json headRefOid --jq .headRefOid 2>/dev/null)
    if [ -n "$sha" ]; then
      printf '%s' "$sha"
      return 0
    fi
  fi
  branch=$(gate_clean "${EPHOR_BRANCH:-}")
  if [ -z "$branch" ]; then
    branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null) || branch=
    [ "$branch" = "HEAD" ] && branch=
  fi
  printf '%s' "$branch"
}

# How a reader would name the thing being asked about, for the summary line.
gate_subject() {
  local repo=$1 ref=$2 number
  number=$(gate_clean "${EPHOR_NUMBER:-}")
  if [ -n "$number" ]; then
    printf '%s#%s' "$repo" "$number"
  else
    printf '%s@%s' "$repo" "$ref"
  fi
}

# Every check run on the ref, as one JSON array. An empty array where the ref
# has no gate at all; non-zero where the forge could not be asked. The two are
# kept apart deliberately: a question nobody could ask has the same shape as a
# gate with nothing red in it, and answering the first as if it were the second
# is a green light on an unbuilt tree. The forge's exit code is what tells
# them apart, so it is read before anything is piped anywhere.
gate_runs() {
  local repo=$1 ref=$2 raw
  raw=$(gh api --paginate "repos/$repo/commits/$ref/check-runs" \
          --jq '.check_runs[] | {name, status, conclusion, id, url: .html_url}' 2>/dev/null) ||
    return 1
  printf '%s' "$raw" | jq -s '.'
}

# The gate's counts, as the answer envelope spells them
# (§FS-006-project-interface.4). Anything unfinished is running; a conclusion
# that is not a pass is a failure, cancelled included — a job somebody stopped
# did not pass, and a gate that counted it as one would be green while the tree
# was not built.
gate_counts() {
  jq -c '{
    running: [.[] | select(.status != "completed")] | length,
    passed:  [.[] | select(.status == "completed"
                           and (.conclusion == "success"
                                or .conclusion == "neutral"
                                or .conclusion == "skipped"))] | length,
    failed:  [.[] | select(.status == "completed"
                           and (.conclusion != null)
                           and (.conclusion != "success")
                           and (.conclusion != "neutral")
                           and (.conclusion != "skipped"))] | length
  }'
}

# The failing check runs, newest information first.
gate_failing() {
  jq -c '[.[] | select(.status == "completed"
                       and (.conclusion != null)
                       and (.conclusion != "success")
                       and (.conclusion != "neutral")
                       and (.conclusion != "skipped"))]'
}

# Write the answer envelope where a program is reading one
# (§FS-006-project-interface.4). Prose stays on stdout for the person; this is
# what a program reads. Nobody reading is not a reason to break the pipe that
# was writing, so the envelope is swallowed rather than refused.
gate_answer() {
  if [ -n "${EPHOR_ANSWER:-}" ]; then
    cat > "$EPHOR_ANSWER"
  else
    cat > /dev/null
  fi
}

# What to say when the forge could not be asked at all. The verbs exit non-zero
# after this: silence read as "green" is the one answer a gate must never give.
gate_unreachable() {
  local subject=$1 why=$2
  echo "The gate of $subject could not be asked: $why" >&2
  jq -n --arg subject "$subject" --arg why "$why" \
    '{v: 1, summary: ("the gate of " + $subject + " could not be asked: " + $why)}' |
    gate_answer
}
