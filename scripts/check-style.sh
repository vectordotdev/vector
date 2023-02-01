#!/usr/bin/env bash
set -euo pipefail

# check-style.sh
#
# SUMMARY
#
#   Checks that all text files have correct line endings and no trailing spaces.

if [ "${1:-}" == "--fix" ]; then
  MODE="fix"
else
  MODE="check"
fi

ised() {
  local PAT="$1"
  local FILE="$2"

  # In-place `sed` that uses the form of `sed -i` which works
  # on both GNU and macOS.
  sed -i.bak "$PAT" "$FILE"
  rm "$FILE.bak"
}

EXIT_CODE=0
for FILE in $(git ls-files); do
  # Ignore binary files and generated files.
  case "$FILE" in
    *png) continue;;
    *svg) continue;;
    *gif) continue;;
    *ico) continue;;
    *sig) continue;;
    *html) continue;;
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
  if grep ' $' "$FILE" > /dev/null; then
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
