#!/usr/bin/env bash
set -euo pipefail

# signoff.sh
#
# SUMMARY
#
#   Signs all previous commits with a DCO signoff as described in the
#   CONTRIBUTING.md document.

export FILTER_BRANCH_SQUELCH_WARNING=true
_current_branch=$(git branch | sed -n -e 's/^\* \(.*\)/\1/p')
hash1=$(git show-ref --heads -s master)
hash2=$(git merge-base master "$_current_branch")

if [ "${hash1}" != "${hash2}" ]; then
  echo "You branch is not rebased with master. Please rebase first:"
  echo ""
  echo "    git rebase master"
  exit 1
fi

echo "We found the following commits since master:"
echo ""

git log master... --pretty=oneline

echo ""
echo -n "Proceed to sign the above commits? (y/n) "

while true; do
  read -r _choice
  case $_choice in
    y) break; ;;
    n) exit; ;;
    *) echo "Please enter y or n"; ;;
  esac
done

echo ""

_signoff="sign: $(git config --get user.name) <$(git config --get user.email)>"
_commit_count=$(git rev-list --count --no-merges master..)

git config trailer.sign.key "Signed-off-by"
git filter-branch -f --msg-filter \
    "git interpret-trailers --trailer \"$_signoff\"" \
    "HEAD~$_commit_count..HEAD"

echo "All done! Your commits have been signed."
echo "In order to update your branch you'll need to force push:"
echo ""
echo "    git push origin $_current_branch --force"
echo ""
