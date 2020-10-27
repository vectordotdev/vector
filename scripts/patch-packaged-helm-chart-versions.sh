#!/usr/bin/env bash
set -euo pipefail

# patch-packaged-helm-chart-versions.sh
#
# SUMMARY
#
#   Take a packaged Helm Chart archive and patch the versions at the subchart
#   manifests.
#
#   This is a gross workaround for one of the Helm's CLI tooling design
#   shortcomings.
#

PACKAGED_CHART_FILE="${1?Pass the archive path as the first argument}"
CHART_NAME="${2?Pass the archive path as the second argument}"
CHART_VERSION="${3?Pass the desired chart version as a third argument}"
APP_VERSION="${4?Pass the desired app version as a fourth argument}"

capture-archive-structure() {
  tar -tf "$PACKAGED_CHART_FILE"
}

# Capture the archive structure before the operation.
STRUCTURE_BEFORE="$(capture-archive-structure)"

# Perform the patching.

TEMPDIR="$(mktemp -d)"
tar -xf "$PACKAGED_CHART_FILE" --directory "$TEMPDIR"
pushd "$TEMPDIR" >/dev/null

mapfile -t CHART_FILES < <(find "$CHART_NAME" -mindepth 2 -name Chart.yaml )
for CHART_FILE in "${CHART_FILES[@]}"; do
  echo "=> Patching versions at $CHART_FILE"

  {
    sed 's/^version: .*$/version: '"$CHART_VERSION"'/g' \
    | sed 's/^appVersion: .*$/appVersion: '"$APP_VERSION"'/g'
  } < "$CHART_FILE" > "$CHART_FILE.new"
  mv "$CHART_FILE.new" "$CHART_FILE"
done

popd >/dev/null
tar -czf "$PACKAGED_CHART_FILE" -C "$TEMPDIR" -T - <<< "$STRUCTURE_BEFORE"
rm -rf "$TEMPDIR"

# Capture the archive structure after the operation.
STRUCTURE_AFTER="$(capture-archive-structure)"

# Check ourselves.
if [[ "$STRUCTURE_AFTER" != "$STRUCTURE_BEFORE" ]]; then
  err() {
    echo "$@" >&2
  }

  err "The archives structure before and after does not match," \
    "something went wrong!"
  err
  err "== Before =="
  err "$STRUCTURE_BEFORE"
  err
  err "== After =="
  err "$STRUCTURE_AFTER"
  err
  exit 1
fi
