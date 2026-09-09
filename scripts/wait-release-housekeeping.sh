#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_REPOSITORY:?}" "${PR_NUMBER:?}" "${HEAD_SHA:?}"

# Checks may not exist yet immediately after opening the PR. Require the release
# validator explicitly, rather than treating an empty required-check list as green.
for ((attempt = 0; attempt < 360; attempt++)); do
  pr=$(gh pr view "$PR_NUMBER" --repo "$GITHUB_REPOSITORY" --json headRefOid,state)
  if [[ "$(jq -r '.headRefOid' <<< "$pr")" != "$HEAD_SHA" ]]; then
    echo "Housekeeping head changed; refusing to merge an unvalidated commit." >&2
    exit 1
  fi
  if [[ "$(jq -r '.state' <<< "$pr")" != OPEN ]]; then
    echo "Housekeeping PR is no longer open." >&2
    exit 1
  fi

  # gh exits nonzero for pending/failing checks as well as for API errors.
  checks=$(gh pr checks "$PR_NUMBER" --repo "$GITHUB_REPOSITORY" \
    --required --json name,bucket 2>&1) || true
  if ! jq -e 'type == "array"' <<< "$checks" >/dev/null 2>&1; then
    # The only expected non-JSON response is the initial absence of checks.
    if [[ "$checks" != "no checks reported"* ]]; then
      echo "Cannot read required checks: $checks" >&2
      exit 1
    fi
  else
    if jq -e 'any(.[]; .bucket == "fail" or .bucket == "cancel")' <<< "$checks" >/dev/null; then
      echo "A required housekeeping check failed: $checks" >&2
      exit 1
    fi
    if jq -e 'any(.[]; .name == "Validate release state transition" and .bucket == "pass")
        and all(.[]; .bucket == "pass" or .bucket == "skipping")' <<< "$checks" >/dev/null; then
      exit 0
    fi
  fi
  sleep 15
done

echo "Timed out waiting for required checks, including Validate release state transition." >&2
exit 1
