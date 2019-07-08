#!/usr/bin/env bash

# signoff.sh
#
# SUMMARY
#
#   Signs all previous commits with a DCO signoff as described in the
#   CONTRIBUTING.md document.


echo "We found the following commits since master:"
echo ""

git log master... --pretty=oneline

echo ""
echo -n "Proceed to sign the above commits? (y/n)"

while true; do
  read _choice
  case $_choice in
    y) break; ;;
    n) exit; ;;
    *) echo "Please enter y or n"; ;;
  esac
done

echo ""

_signoff="sign: $(git config --get user.name) <$(git config --get user.email)>"
_commit_count=$(git rev-list --count --no-merges master..)
_current_branch=$(git branch | sed -n -e 's/^\* \(.*\)/\1/p')

git config trailer.sign.key "Signed-off-by"
git filter-branch -f --msg-filter \
    "git interpret-trailers --trailer \"$_signoff\"" \
     HEAD~$_commit_count..HEAD

echo "All done! Your commits have been signed."
echo "In order to update your branch you'll need to force push:"
echo ""
echo "    git push origin $_current_branch --force"
echo ""