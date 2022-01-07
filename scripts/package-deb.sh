#!/usr/bin/env bash
set -xe

#
# Local vars
#

PROJECT_ROOT="$(pwd)"
# PACKAGE_VERSION="${VECTOR_VERSION:local}"
# TARGET=$(rustup target list | grep installed | head -n 1 | cut -d ' ' -f 1)

#
# Package
#

mkdir -p $PROJECT_ROOT/target/deb

# Create short plain-text extended description for the package
EXPANDED_LINK_ALIASED="$(cmark-gfm "$PROJECT_ROOT/README.md" --to commonmark)" # expand link aliases
TEXT_BEFORE_FIRST_HEADER="$(sed '/^## /Q' <<< "$EXPANDED_LINK_ALIASED")" # select text before first header
PLAIN_TEXT="$(cmark-gfm --to plaintext <<< "$TEXT_BEFORE_FIRST_HEADER")" # convert to plain text
FORMATTED="$(fmt -uw 80 <<< "$PLAIN_TEXT")"
cat <<< "$FORMATTED" > "$PROJECT_ROOT/target/debian-extended-description.txt"

# Create the license file for binary distributions (LICENSE + NOTICE)
cat LICENSE NOTICE > "$PROJECT_ROOT/target/debian-license.txt"

cargo deb --output $PROJECT_ROOT/target/deb/

