#/bin/bash

# This script is intended to run during CI, however it can be run locally by
# committing changelog fragments before executing the script. If the script
# finds an issue with your changelog fragment, you can unstage the fragment
# from being committed and fix the issue.

CHANGELOG_DIR="changelog.d"
FRAGMENT_TYPES="breaking|security|deprecated|feature|enhanced|fixed"

if [ ! -d "${CHANGELOG_DIR}" ]; then
  echo "No ./${CHANGELOG_DIR} found. This tool must be invoked from the root of the OPW repo."
  exit 1
fi

# diff-filter=A lists only added files
ADDED=$(git diff --name-only --diff-filter=A origin/master ${CHANGELOG_DIR})

if [ $(echo "$ADDED" | grep -c .) -lt 1 ]; then
  echo "No changelog fragments detected"
  echo "If no changes  necessitate user-facing explanations, add the GH label 'no-changelog'"
  echo "Otherwise, add changelog fragments to changelog.d/"
  exit 1
fi

# extract the basename from the file path
ADDED=$(echo ${ADDED} | xargs -n1 basename)

# validate the fragments
while IFS= read -r fname; do

  if [[ ${fname} == "README.md" ]]; then
    continue
  fi

  echo "validating '${fname}'"

  arr=(${fname//./ })

  if [ "${#arr[@]}" -ne 3 ]; then
    echo "invalid fragment filename: wrong number of period delimiters. expected '<pr_number>.<fragment_type>.md'. (${fname})"
    exit 1
  fi

  if ! [[ "${arr[0]}" =~ ^[0-9]+$ ]]; then
    echo "invalid fragment filename: fragment must begin with an integer (PR number). expected '<pr_number>.<fragment_type>.md' (${fname})"
    exit 1
  fi

  if ! [[ "${arr[1]}" =~ ^(${FRAGMENT_TYPES})$ ]]; then
    echo "invalid fragment filename: fragment type must be one of: (${FRAGMENT_TYPES}). (${fname})"
    exit 1
  fi

  if [[ "${arr[2]}" != "md" ]]; then
    echo "invalid fragment filename: extension must be markdown (.md): (${fname})"
    exit 1
  fi

  # if specified, this option validates that the contents of the news fragment
  # contains a properly formatted authors line at the end of the file, generally
  # used for external contributor PRs.
  if [[ $1 == "--authors" ]]; then
    last=$( tail -n 1 "${CHANGELOG_DIR}/${fname}" )
    if ! [[ "${last}" =~ ^(authors: .*)$ ]]; then
      echo "invalid fragment contents: author option was specified but fragment ${fname} contains no authors."
      exit 1
    fi

  fi

done <<< "$ADDED"


echo "changelog additions are valid."
