#!/bin/bash
# Refresh files already vendored from a git repository at a release tag.
# An explicit tag is used as given. With no tag, the highest tag matching
# tag_regex is used.
set -o errexit
set -o nounset
set -o pipefail

if [[ $# -lt 6 || $# -gt 7 ]]; then
  echo "usage: $0 repo dest src_rel tag_regex title script_name [tag]" >&2
  exit 1
fi

repo=$1
dest=$2
src_rel=$3
tag_regex=$4
title=$5
script_name=$6
shift 6

if [[ $# -eq 1 ]]; then
  tag=$1
else
  tag=$(
    git ls-remote --tags --refs "$repo" \
      | awk '{print $2}' \
      | sed 's#^refs/tags/##' \
      | grep -E "$tag_regex" \
      | sort -V \
      | tail -n 1
  )
fi

# Drop repository variables inherited from the caller. Otherwise git
# operates on that repository and looks for the tag as one of its branches.
unset GIT_DIR GIT_WORK_TREE GIT_PREFIX GIT_COMMON_DIR GIT_INDEX_FILE GIT_OBJECT_DIRECTORY

checkout=$(mktemp --directory --tmpdir="$PWD")
trap 'rm -fr "$checkout"' EXIT
# -c applies only to this command, so the detached-HEAD notice from a tag
# checkout is skipped without writing git config.
git -c advice.detachedHead=false clone --depth 1 --branch "$tag" --filter blob:none "$repo" "$checkout"

src=$checkout/$src_rel
while IFS= read -r -d '' path; do
  rel=${path#"$dest"/}
  if [[ $rel == README.md ]]; then
    continue
  fi
  cp --verbose "$src/$rel" "$path"
  # `vdev check fmt` rejects a non-empty file that does not end in a newline.
  if [[ -s $path && $(tail -c 1 "$path") != $'\n' ]]; then
    echo >> "$path"
  fi
done < <(find "$dest" -type f -print0)

repo_slug=${repo#https://github.com/}
cat > "$dest/README.md" <<EOF
# ${title}

Vendored from [\`${repo_slug}\`](${repo})
[\`${src_rel}\`](${repo}/tree/${tag}/${src_rel})
at release [\`${tag}\`](${repo}/releases/tag/${tag}).

Files already present in this directory are refreshed from that release.

Refresh by running \`scripts/${script_name}\` for the latest release,
or \`scripts/${script_name} ${tag}\` to pin this tag again.
EOF
