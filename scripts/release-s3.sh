#!/usr/bin/env bash
set -euo pipefail

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

vdev_cmd="${VDEV:-cargo vdev}"

CHANNEL="${CHANNEL:-"$($vdev_cmd release channel)"}"
VERSION="${VECTOR_VERSION:-"$($vdev_cmd version)"}"
DATE="${DATE:-"$(date -u +%Y-%m-%d)"}"
VERIFY_TIMEOUT="${VERIFY_TIMEOUT:-"30"}" # seconds
VERIFY_RETRIES="${VERIFY_RETRIES:-"2"}"

export AWS_REGION=us-east-1

COSE_BUCKET="dd-cose-releases"
COSE_PUBLIC_URL="https://${COSE_BUCKET}.s3.amazonaws.com"

s3_copy() {
    aws s3 cp "$@" --sse
}

publish_release_aliases() {
    local version_prefix="$1"
    local file

    find "$td" -maxdepth 1 -type f -print0 | while read -r -d $'\0' file; do
        file_name=$(basename "$file")
        # S3's REST endpoint does not follow website redirects, so COSE aliases
        # are full copies of the corresponding release artifact.
        s3_copy "$file" "s3://$COSE_BUCKET/vector/$version_prefix/${file_name/-$VERSION_EXACT/}"
    done
}

publish_release_artifacts() {
    local version_prefix="$1"

    echo "Uploading artifacts to s3://$COSE_BUCKET/vector/$version_prefix/"
    s3_copy "$td" "s3://$COSE_BUCKET/vector/$version_prefix/" --recursive

    echo "Adding versionless artifact names in s3://$COSE_BUCKET/vector/$version_prefix/"
    publish_release_aliases "$version_prefix"
    echo "Added versionless artifact names"
}

#
# Setup
#

td="$(mktemp -d)"
cp -av "target/artifacts/." "$td"

td_nightly="$(mktemp -d)"
cp -av "target/artifacts/." "$td_nightly"

for f in "$td_nightly"/*; do
    # RPM artifacts use `~` instead of `-` in their version (see
    # scripts/package-rpm.sh), so substitute both spellings when renaming
    # artifacts for the nightly bucket.
    a="$(echo "$f" | sed -r -e "s/$VERSION/nightly/" -e "s/${VERSION//-/\~}/nightly/")"
    [[ "$a" != "$f" ]] && mv "$f" "$a"
done
ls "$td_nightly"

#
# A helper function for verifying a published artifact.
#
# Retries transient download failures and content mismatches while a newly
# uploaded object becomes publicly readable.
verify_artifact() {
  local URL="$1"
  local FILENAME="$2"
  local attempts=7
  local delay=1
  echo "Verifying $URL"
  for ((attempt = 1; attempt <= attempts; attempt++)); do
    if cmp <(wget -qO- --retry-on-http-error=404 --wait 10 --tries "$VERIFY_RETRIES" "$URL") "$FILENAME"; then
      return 0
    fi
    if (( attempt < attempts )); then
      echo "Attempt $attempt/$attempts did not match (likely stale CDN cache); retrying in ${delay}s"
      sleep "$delay"
      delay=$((delay * 2))
    fi
  done
  echo "Verification of $URL failed after $attempts attempts"
  return 1
}

#
# Upload
#

if [[ "$CHANNEL" == "nightly" ]]; then
  # Add nightly files with the $DATE for posterity
  echo "Uploading all artifacts to s3://$COSE_BUCKET/vector/nightly/$DATE"
  s3_copy "$td_nightly" "s3://$COSE_BUCKET/vector/nightly/$DATE" --recursive
  echo "Uploaded archives"

  # Add "latest" nightly files
  echo "Uploading all artifacts to s3://$COSE_BUCKET/vector/nightly/latest"
  s3_copy "$td_nightly" "s3://$COSE_BUCKET/vector/nightly/latest" --recursive
  echo "Uploaded archives"

  # Verify that the files exist and can be downloaded
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  sleep "$VERIFY_TIMEOUT"
  verify_artifact \
    "$COSE_PUBLIC_URL/vector/nightly/$DATE/vector-nightly-x86_64-unknown-linux-musl.tar.gz" \
    "$td_nightly/vector-nightly-x86_64-unknown-linux-musl.tar.gz"
  verify_artifact \
    "$COSE_PUBLIC_URL/vector/nightly/latest/vector-nightly-x86_64-unknown-linux-musl.tar.gz" \
    "$td_nightly/vector-nightly-x86_64-unknown-linux-musl.tar.gz"
  verify_artifact \
    "$COSE_PUBLIC_URL/vector/nightly/latest/vector-nightly-x86_64-unknown-linux-gnu.tar.gz" \
    "$td_nightly/vector-nightly-x86_64-unknown-linux-gnu.tar.gz"

elif [[ "$CHANNEL" == "release" ]]; then
  VERSION_EXACT="$VERSION"

  # COSE exposes stable artifacts only under their immutable exact version.
  publish_release_artifacts "$VERSION_EXACT"

  # Verify that the files exist and can be downloaded
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  sleep "$VERIFY_TIMEOUT"
  verify_artifact \
    "$COSE_PUBLIC_URL/vector/$VERSION_EXACT/vector-$VERSION-x86_64-unknown-linux-musl.tar.gz" \
    "$td/vector-$VERSION-x86_64-unknown-linux-musl.tar.gz"

elif [[ "$CHANNEL" == "custom" ]]; then

  # Add custom files
  echo "Uploading all artifacts to s3://$COSE_BUCKET/vector/custom"
  s3_copy "$td" "s3://$COSE_BUCKET/vector/custom/$VERSION" --recursive
  echo "Uploaded archives"

  # Verify that the files exist and can be downloaded
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  sleep "$VERIFY_TIMEOUT"
  verify_artifact \
    "$COSE_PUBLIC_URL/vector/custom/$VERSION/vector-$VERSION-x86_64-unknown-linux-gnu.tar.gz" \
    "$td/vector-$VERSION-x86_64-unknown-linux-gnu.tar.gz"

fi

#
# Cleanup
#

rm -rf "$td"
rm -rf "$td_nightly"
