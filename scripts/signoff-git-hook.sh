#!/bin/bash
set -euo pipefail

# Used by vdev/src/commands/install_git_hooks.rs to
# automatically sign off your commits.
#
# Installation:
#
#    cp scripts/signoff-git-hook.sh .git/hooks/commit-msg
#
# It's also possible to symlink the script, however that's a security hazard and
# is not recommended.

NAME="$(git config user.name)"
EMAIL="$(git config user.email)"

if [ -z "$NAME" ]; then
  echo "empty git config user.name"
  exit 1
fi

if [ -z "$EMAIL" ]; then
  echo "empty git config user.email"
  exit 1
fi

git interpret-trailers --if-exists doNothing --trailer \
  "Signed-off-by: $NAME <$EMAIL>" \
  --in-place "$1"
