#!/bin/bash
set -eo pipefail

# check-style.sh
#
# SUMMARY
#
#   Checks that all text files have correct line endings and no trailing spaces.

exit_code=0
if [ "$1" == "--fix" ]; then
  mode="fix"
else
  mode="check"
fi

function ised() {
  # In-place `sed` that uses the form of `sed -i` which works
  # on both GNU and macOS.
  sed -i.bak "$1" "$2"
  rm "$2.bak"
}

for i in $(git ls-files); do
  # ignore binary files
  case $i in
    *png) continue;;
    *svg) continue;;
    *ico) continue;;
    *sig) continue;;
    test-data*) continue;;
    tests/data*) continue;;
    website/plugins/*) continue;;
    website/sidebars.js) continue;;
  esac

  # check that the file contains trailing newline
  if [ -n "$(tail -c1 $i | tr -d $'\n')" ]; then
    case $mode in
      check)
        echo "File \"$i\" doesn't end with a newline"
        exit_code=1
        ;;
      fix)
        echo >> $i
        ;;
    esac
  fi

  # check that the file uses LF line breaks
  if grep $'\r$' $i > /dev/null; then
    case $mode in
      check)
        echo "File \"$i\" contains CRLF line breaks instead of LF line breaks"
        exit_code=1
        ;;
      fix)
        ised 's/\r$//' $i
        ;;
    esac
  fi

  # check that the lines don't contain trailing spaces
  if grep ' $' $i > /dev/null; then
    case $mode in
      check)
        echo "File \"$i\" contains trailing spaces in some of the lines"
        exit_code=1
        ;;
      fix)
        ised 's/ *$//' $i
        ;;
    esac
  fi
done
exit $exit_code
