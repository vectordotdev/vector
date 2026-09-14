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

LEGACY_BUCKET="packages.timber.io"
COSE_BUCKET="dd-cose-releases"
COSE_PUBLIC_URL="https://${COSE_BUCKET}.s3.amazonaws.com"

# Stable releases are dual-published with legacy static credentials and the
# COSE GitHub OIDC credentials configured by the publish workflow.
legacy_aws() {
    : "${LEGACY_AWS_ACCESS_KEY_ID:?LEGACY_AWS_ACCESS_KEY_ID is required for legacy stable releases}"
    : "${LEGACY_AWS_SECRET_ACCESS_KEY:?LEGACY_AWS_SECRET_ACCESS_KEY is required for legacy stable releases}"
    env -u AWS_SESSION_TOKEN -u AWS_SECURITY_TOKEN \
        AWS_ACCESS_KEY_ID="$LEGACY_AWS_ACCESS_KEY_ID" \
        AWS_SECRET_ACCESS_KEY="$LEGACY_AWS_SECRET_ACCESS_KEY" \
        aws "$@"
}

cose_aws() {
    aws "$@"
}

s3_copy() {
    local client="$1"
    shift

    if [[ "$client" == "legacy_aws" ]]; then
        "$client" s3 cp "$@" --sse --acl public-read
    else
        "$client" s3 cp "$@" --sse
    fi
}

publish_release_redirects() {
    local client="$1"
    local bucket="$2"
    local version_prefix="$3"
    local file

    if [[ "$client" == "legacy_aws" ]]; then
        for file in $("$client" s3api list-objects-v2 --bucket "$bucket" --prefix "vector/$version_prefix/" --query 'Contents[*].Key' --output text | tr "\t" "\n" | grep "\-$VERSION_EXACT"); do
            file=$(basename "$file")
            # vector-$version-amd64.deb -> vector-amd64.deb
            echo -n "" | s3_copy "$client" - "s3://$bucket/vector/$version_prefix/${file/-$VERSION_EXACT/}" --website-redirect "/vector/$version_prefix/$file"
        done
    else
        find "$td" -maxdepth 1 -type f -print0 | while read -r -d $'\0' file; do
            file_name=$(basename "$file")
            # S3's REST endpoint does not follow website redirects, so COSE
            # aliases are full copies of the corresponding release artifact.
            s3_copy "$client" "$file" "s3://$bucket/vector/$version_prefix/${file_name/-$VERSION_EXACT/}"
        done
    fi
}

publish_release_artifacts() {
    local client="$1"
    local bucket="$2"
    local version_prefix="$3"

    echo "Uploading artifacts to s3://$bucket/vector/$version_prefix/"
    s3_copy "$client" "$td" "s3://$bucket/vector/$version_prefix/" --recursive

    # `latest` is mutable in both buckets, so remove aliases that are no longer
    # produced by the current build. Only legacy publishes mutable X prefixes.
    if [[ "$version_prefix" == "latest" ]] ||
       [[ "$client" == "legacy_aws" &&
          ( "$version_prefix" == "${VERSION_MAJOR_X}" || "$version_prefix" == "${VERSION_MINOR_X}" ) ]] ; then
        echo "Deleting old artifacts from s3://$bucket/vector/$version_prefix/"
        "$client" s3 rm "s3://$bucket/vector/$version_prefix/" --recursive --exclude "*$VERSION_EXACT*"
        echo "Deleted old versioned artifacts"
    fi

    echo "Redirecting old artifact names in s3://$bucket/vector/$version_prefix/"
    publish_release_redirects "$client" "$bucket" "$version_prefix"
    echo "Redirected old artifact names"
}

#
# Setup
#

td="$(mktemp -d)"
cp -av "target/artifacts/." "$td"

td_nightly="$(mktemp -d)"
cp -av "target/artifacts/." "$td_nightly"

for f in "$td_nightly"/*; do
    a="$(echo "$f" | sed -r -e "s/$VERSION/nightly/")"
    mv "$f" "$a"
done
ls "$td_nightly"

#
# A helper function for verifying a published artifact.
#
# Retries a content mismatch as well as a 404, since packages.timber.io is
# fronted by a CDN and an object we just overwrote via `aws s3 rm` + `cp` can
# serve stale bytes at the edge for a while.
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
  s3_copy cose_aws "$td_nightly" "s3://$COSE_BUCKET/vector/nightly/$DATE" --recursive
  echo "Uploaded archives"

  # Add "latest" nightly files
  echo "Uploading all artifacts to s3://$COSE_BUCKET/vector/nightly/latest"
  s3_copy cose_aws "$td_nightly" "s3://$COSE_BUCKET/vector/nightly/latest" --recursive
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
  # shellcheck disable=SC2001
  VERSION_MINOR_X="$(echo "$VERSION" | sed 's/\.[0-9]*$/.X/g')"
  # shellcheck disable=SC2001
  VERSION_MAJOR_X="$(echo "$VERSION" | sed 's/\.[0-9]*\.[0-9]*$/.X/g')"

  # Preserve all existing release paths in the legacy bucket during the
  # migration window.
  for i in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X" "latest"; do
    publish_release_artifacts legacy_aws "$LEGACY_BUCKET" "$i"
  done

  # COSE exposes stable artifacts only under their immutable exact version.
  publish_release_artifacts cose_aws "$COSE_BUCKET" "$VERSION_EXACT"

  echo "Add latest symlinks"
  find "$td" -maxdepth 1 -type f -print0 | while read -r -d $'\0' file ; do
    file=$(basename "$file")
    # vector-$version-amd64.deb -> vector-latest-amd64.deb
    echo -n "" | s3_copy legacy_aws - "s3://$LEGACY_BUCKET/vector/latest/${file/$VERSION_EXACT/latest}" --website-redirect "/vector/latest/$file"
    # vector-$version-amd64.deb -> vector-amd64.deb
    echo -n "" | s3_copy legacy_aws - "s3://$LEGACY_BUCKET/vector/latest/${file/$VERSION_EXACT-/}" --website-redirect "/vector/latest/$file"
  done
  echo "Added latest symlinks"

  # Verify that the files exist and can be downloaded
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  sleep "$VERIFY_TIMEOUT"
  for i in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X" "latest"; do
    verify_artifact \
      "https://packages.timber.io/vector/$i/vector-$VERSION-x86_64-unknown-linux-musl.tar.gz" \
      "$td/vector-$VERSION-x86_64-unknown-linux-musl.tar.gz"
  done
  verify_artifact \
    "$COSE_PUBLIC_URL/vector/$VERSION_EXACT/vector-$VERSION-x86_64-unknown-linux-musl.tar.gz" \
    "$td/vector-$VERSION-x86_64-unknown-linux-musl.tar.gz"
  verify_artifact \
    "https://packages.timber.io/vector/latest/vector-latest-x86_64-unknown-linux-gnu.tar.gz" \
    "$td/vector-$VERSION-x86_64-unknown-linux-gnu.tar.gz"

elif [[ "$CHANNEL" == "custom" ]]; then

  # Add custom files
  echo "Uploading all artifacts to s3://$COSE_BUCKET/vector/custom"
  s3_copy cose_aws "$td" "s3://$COSE_BUCKET/vector/custom/$VERSION" --recursive
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
