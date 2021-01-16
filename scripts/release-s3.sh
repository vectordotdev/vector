#!/usr/bin/env bash
set -euo pipefail

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

CHANNEL="${CHANNEL:-"$(scripts/release-channel.sh)"}"
VERSION="${VERSION:-"$(scripts/version.sh)"}"
DATE="${DATE:-"$(date -u +%Y-%m-%d)"}"
VERIFY_TIMEOUT="${VERIFY_TIMEOUT:-"30"}" # seconds
VERIFY_RETRIES="${VERIFY_RETRIES:-"2"}"

#
# Setup
#

td="$(mktemp -d)"
cp -av "target/artifacts/." "$td"
ls "$td"

td_nightly="$(mktemp -d)"
cp -av "target/artifacts/." "$td_nightly"

for f in "$td_nightly"/*; do
    a="$(echo "$f" | sed -r -e "s/$VERSION/nightly/")"
    mv "$f" "$a"
done
ls "$td_nightly"

td_latest="$(mktemp -d)"
cp -av "target/artifacts/." "$td_latest"

for f in "$td_latest"/*; do
    a="$(echo "$f" | sed -r -e "s/$VERSION/latest/")"
    mv "$f" "$a"
done
ls "$td_latest"

#
# A helper function for verifying a published artifact.
#
verify_artifact() {
  local URL="$1"
  local FILENAME="$2"
  echo "Verifying $URL"
  cmp <(wget -qO- --retry-on-http-error=404 --wait 10 --tries "$VERIFY_RETRIES" "$URL") "$FILENAME"
}

#
# Upload
#

if [[ "$CHANNEL" == "nightly" ]]; then
  # Add nightly files with the $DATE for posterity
  echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/$DATE"
  aws s3 cp "$td_nightly" "s3://packages.timber.io/vector/nightly/$DATE" --recursive --sse --acl public-read
  echo "Uploaded archives"

  # Add "latest" nightly files
  echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/latest"
  aws s3 rm --recursive "s3://packages.timber.io/vector/nightly/latest"
  aws s3 cp "$td_nightly" "s3://packages.timber.io/vector/nightly/latest" --recursive --sse --acl public-read
  echo "Uploaded archives"

  echo "Redirecting old artifact names"
  find "$td_nightly" -maxdepth 1 -type f -print0 | while read -r -d $'\0' file  ; do
    file=$(basename "$file")
    # vector-nightly-amd64.deb -> vector-amd64.deb
    echo -n "" | aws s3 cp - "s3://packages.timber.io/vector/nightly/$DATE/${file/-nightly/}" --website-redirect "/vector/nightly/$DATE/$file" --acl public-read
    echo -n "" | aws s3 cp - "s3://packages.timber.io/vector/nightly/latest/${file/-nightly/}" --website-redirect "/vector/nightly/latest/$file" --acl public-read
  done
  echo "Redirected old artifact names"

  # Verify that the files exist and can be downloaded
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  sleep "$VERIFY_TIMEOUT"
  verify_artifact \
    "https://packages.timber.io/vector/nightly/$DATE/vector-nightly-x86_64-unknown-linux-musl.tar.gz" \
    "$td_nightly/vector-nightly-x86_64-unknown-linux-musl.tar.gz"
  verify_artifact \
    "https://packages.timber.io/vector/nightly/latest/vector-nightly-x86_64-unknown-linux-musl.tar.gz" \
    "$td_nightly/vector-nightly-x86_64-unknown-linux-musl.tar.gz"
  verify_artifact \
    "https://packages.timber.io/vector/nightly/latest/vector-nightly-x86_64-unknown-linux-gnu.tar.gz" \
    "$td_nightly/vector-nightly-x86_64-unknown-linux-gnu.tar.gz"
elif [[ "$CHANNEL" == "latest" ]]; then
  VERSION_EXACT="$VERSION"
  # shellcheck disable=SC2001
  VERSION_MINOR_X="$(echo "$VERSION" | sed 's/\.[0-9]*$/.X/g')"
  # shellcheck disable=SC2001
  VERSION_MAJOR_X="$(echo "$VERSION" | sed 's/\.[0-9]*\.[0-9]*$/.X/g')"

  for i in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X"; do
    # Upload the specific version
    echo "Uploading artifacts to s3://packages.timber.io/vector/$i/"
    aws s3 cp "$td" "s3://packages.timber.io/vector/$i/" --recursive --sse --acl public-read
  done

  for i in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X"; do
    # Delete anything that isn't the current version
    echo "Deleting old artifacts from s3://packages.timber.io/vector/$i/"
    aws s3 rm "s3://packages.timber.io/vector/$i/" --exclude "*$VERSION_EXACT*"
    echo "Deleted old versioned artifacts"
  done

  echo "Uploading artifacts to s3://packages.timber.io/vector/latest/"
  aws s3 cp "$td_latest" "s3://packages.timber.io/vector/latest/" --recursive --sse --acl public-read
  echo "Uploaded latest archives"

  echo "Redirecting old artifact names"
  find "$td" -maxdepth 1 -type f -print0 | while read -r -d $'\0' file  ; do
    file=$(basename "$file")
    # vector-$version-amd64.deb -> vector-amd64.deb
    echo -n "" | aws s3 cp - "s3://packages.timber.io/vector/$i/${file/-$i/}" --website-redirect "/vector/$i/$file" --acl public-read
    echo -n "" | aws s3 cp - "s3://packages.timber.io/vector/latest/${file/-$i/}" --website-redirect "/vector/latest/$file" --acl public-read
  done
  echo "Redirected old artifact names"

  # Verify that the files exist and can be downloaded
  sleep "$VERIFY_TIMEOUT"
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  for i in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X"; do
    verify_artifact \
      "https://packages.timber.io/vector/$i/vector-$VERSION-x86_64-unknown-linux-musl.tar.gz" \
      "$td/vector-$VERSION-x86_64-unknown-linux-musl.tar.gz"
  done
  verify_artifact \
    "https://packages.timber.io/vector/latest/vector-latest-x86_64-unknown-linux-gnu.tar.gz" \
    "$td_latest/vector-latest-x86_64-unknown-linux-gnu.tar.gz"
fi

#
# Cleanup
#

rm -rf "$td"
rm -rf "$td_nightly"
rm -rf "$td_latest"
