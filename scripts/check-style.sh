#!/usr/bin/env bash
set -euo pipefail

# check-style.sh
#
# SUMMARY
#
#   Checks that all text files have correct line endings and no trailing spaces.
#
# USAGE
#
#   ./scripts/check-style.sh [--fix] [--all]
#
#   --fix: Fix issues instead of just reporting them
#   --all: Check all files (default: only check modified files)

MODE="check"
CHECK_ALL=false

# Parse arguments
for arg in "$@"; do
  case "$arg" in
    --fix)
      MODE="fix"
      ;;
    --all)
      CHECK_ALL=true
      ;;
    *)
      echo "Unknown option: $arg"
      echo "Usage: $0 [--fix] [--all]"
      exit 1
      ;;
  esac
done

ised() {
  local PAT="$1"
  local FILE="$2"

  # In-place `sed` that uses the form of `sed -i` which works
  # on both GNU and macOS.
  sed -i.bak "$PAT" "$FILE"
  rm "$FILE.bak"
}

# Determine which files to check
if [ "$CHECK_ALL" = true ]; then
  # Check all files tracked by git
  FILES=$(git ls-files)
else
  # Check only files changed in current branch compared to origin/master
  FILES=$(git diff --name-only "origin/master"...HEAD)

  # If no changed files, fall back to checking all files
  if [ -z "$FILES" ]; then
    FILES=$(git ls-files)
  fi
fi

EXIT_CODE=0
for FILE in $FILES; do
  # Ignore binary files and generated files.
  case "$FILE" in
    *png) continue;;
    *svg) continue;;
    *gif) continue;;
    *ico) continue;;
    *sig) continue;;
    *html) continue;;
    *desc) continue;;
    tests/data*) continue;;
    lib/codecs/tests/data*) continue;;
    lib/vector-core/tests/data*) continue;;
    distribution/kubernetes/*/*.yaml) continue;;
    tests/helm-snapshots/*/snapshot.yaml) continue;;
    lib/remap-tests/tests/*.vrl) continue;;
    lib/datadog/grok/patterns/*.pattern) continue;;
  esac

  # Skip all directories (usually this only happens when we have symlinks).
  if [[ -d "$FILE" ]]; then
    continue
  fi

  # Skip files that don't exist (e.g., deleted in this branch).
  if [[ ! -f "$FILE" ]]; then
    continue
  fi

  # check that the file contains trailing newline
  if [ -n "$(tail -c1 "$FILE" | tr -d $'\n')" ]; then
    case "$MODE" in
      check)
        echo "File \"$FILE\" doesn't end with a newline"
        EXIT_CODE=1
        ;;
      fix)
        echo >> "$FILE"
        ;;
    esac
  fi

  # check that the file uses LF line breaks
  if grep $'\r$' "$FILE" > /dev/null; then
    case "$MODE" in
      check)
        echo "File \"$FILE\" contains CRLF line breaks instead of LF line breaks"
        EXIT_CODE=1
        ;;
      fix)
        ised 's/\r$//' "$FILE"
        ;;
    esac
  fi

  # check that the lines don't contain trailing spaces
  if grep -n ' $' "$FILE"; then
    case "$MODE" in
      check)
        echo "File \"$FILE\" contains trailing spaces in some of the lines"
        EXIT_CODE=1
        ;;
      fix)
        ised 's/ *$//' "$FILE"
        ;;
    esac
  fi
done

exit "$EXIT_CODE"
