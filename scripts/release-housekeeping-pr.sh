#!/usr/bin/env bash
set -euo pipefail

# Run on freshly checked-out master after generating housekeeping and dependency
# files. Rebuild from master on every retry; never trust an existing PR's tree.
: "${VERSION:?}" "${NEXT_VERSION:?}" "${BOT_APP:?}" "${GITHUB_REPOSITORY:?}"
branch="release/housekeeping-v${VERSION}"
base=$(git rev-parse HEAD)
gh auth setup-git

prs=$(gh api --method GET "repos/${GITHUB_REPOSITORY}/pulls" \
  -f state=open -f base=master -f head="${GITHUB_REPOSITORY%/*}:${branch}")
jq -e --arg author "${BOT_APP}[bot]" --arg repo "$GITHUB_REPOSITORY" \
  'length <= 1 and all(.[]; .user.login == $author and .head.repo.full_name == $repo)' \
  <<< "$prs" >/dev/null

# Only stage the generated release files, not unrelated working-tree changes.
git add Cargo.toml Cargo.lock .github/release-state.json LICENSE-3rdparty.csv docs/generated/
tree=$(git write-tree)
git config user.name "${BOT_APP}[bot]"
git config user.email "${BOT_APP}[bot]@users.noreply.github.com"

parents=(-p "$base")
status=0
git ls-remote --exit-code --heads origin "$branch" >/dev/null || status=$?
if [[ "$status" == 0 ]]; then
  git fetch origin "refs/heads/${branch}"
  previous=$(git rev-parse FETCH_HEAD)
  # Append a merge commit with the freshly generated tree. This preserves PR
  # history and incorporates current master without a force push.
  parents=(-p "$previous" -p "$base")
  if [[ "$(git rev-parse "${previous}^{tree}")" == "$tree" ]]; then
    sha="$previous"
  fi
elif [[ "$status" != 2 ]]; then
  exit "$status"
fi
sha=${sha:-$(git commit-tree "$tree" "${parents[@]}" -m "chore(releasing): begin ${NEXT_VERSION}")}
git reset --soft "$sha"
cargo vdev release workflow pr-check --base-sha "$base" --head-ref "$branch"

git push origin "${sha}:refs/heads/${branch}"
number=$(jq -r '.[0].number // empty' <<< "$prs")
if [[ -z "$number" ]]; then
  url=$(gh pr create --repo "$GITHUB_REPOSITORY" --base master --head "$branch" \
    --title "chore(releasing): begin ${NEXT_VERSION}" --label no-changelog \
    --body "Housekeeping after v${VERSION}: begin ${NEXT_VERSION}, restore VRL main, and refresh dependency files. Merged automatically only after required checks pass.")
  number=${url##*/}
fi
printf 'pr_number=%s\nhead_sha=%s\n' "$number" "$sha" >> "$GITHUB_OUTPUT"
echo "Housekeeping PR #${number} at ${sha}." >> "$GITHUB_STEP_SUMMARY"
