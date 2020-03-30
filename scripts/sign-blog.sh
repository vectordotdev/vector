#!/bin/bash

# sign-blog.rb
#
# SUMMARY
#
#   Adds detached GPG signatures to blog articles which
#   don't have these signatures yet.

cd "$(dirname "$0")/../website/blog"

for i in *.md; do
  if [ -f $i.sig ]; then
    continue
  fi

  gpg --detach-sign $i
done
