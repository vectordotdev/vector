#!/bin/bash
set -euo pipefail

# sign-blog.rb
#
# SUMMARY
#
#   Adds detached GPG signatures to blog articles which
#   don't have these signatures yet.

cd "$(dirname "${BASH_SOURCE[0]}")/../website/blog"

if [[ -n "${ARTICLE:-}" ]]; then
  rm "${ARTICLE}.md.sig"
  gpg --detach-sign "${ARTICLE}.md"
else
  for ARTICLE in *.md; do
    if [ -f "$ARTICLE.sig" ]; then
      continue
    fi

    gpg --detach-sign "$ARTICLE"
  done
fi
