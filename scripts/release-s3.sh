#!/usr/bin/env bash
set -euo pipefail

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

CHANNEL="${CHANNEL:-"$(cargo vdev release channel)"}"
VERSION="${VECTOR_VERSION:-"$(cargo vdev version)"}"
DATE="${DATE:-"$(date -u +%Y-%m-%d)"}"
VERIFY_TIMEOUT="${VERIFY_TIMEOUT:-"30"}" # seconds
VERIFY_RETRIES="${VERIFY_RETRIES:-"2"}"

export AWS_REGION=us-east-1

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
  for file in $(aws s3api list-objects-v2 --bucket packages.timber.io --prefix "vector/$i/" --query 'Contents[*].Key' --output text  | tr "\t" "\n" | grep '\-nightly'); do
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

elif [[ "$CHANNEL" == "release" ]]; then
  VERSION_EXACT="$VERSION"
  # shellcheck disable=SC2001
  VERSION_MINOR_X="$(echo "$VERSION" | sed 's/\.[0-9]*$/.X/g')"
  # shellcheck disable=SC2001
  VERSION_MAJOR_X="$(echo "$VERSION" | sed 's/\.[0-9]*\.[0-9]*$/.X/g')"

  for i in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X" "latest"; do
    # Upload the specific version
    echo "Uploading artifacts to s3://packages.timber.io/vector/$i/"
    aws s3 cp "$td" "s3://packages.timber.io/vector/$i/" --recursive --sse --acl public-read

    if [[ "$i" == "${VERSION_MAJOR_X}" || "$i" == "${VERSION_MINOR_X}" || "$i" == "latest" ]] ; then
      # Delete anything that isn't the current version
      echo "Deleting old artifacts from s3://packages.timber.io/vector/$i/"
      aws s3 rm "s3://packages.timber.io/vector/$i/" --recursive --exclude "*$VERSION_EXACT*"
      echo "Deleted old versioned artifacts"
    fi

    echo "Redirecting old artifact names in s3://packages.timber.io/vector/$i/"
    for file in $(aws s3api list-objects-v2 --bucket packages.timber.io --prefix "vector/$i/" --query 'Contents[*].Key' --output text  | tr "\t" "\n" | grep "\-$VERSION_EXACT"); do
      file=$(basename "$file")
      # vector-$version-amd64.deb -> vector-amd64.deb
      echo -n "" | aws s3 cp - "s3://packages.timber.io/vector/$i/${file/-$VERSION_EXACT/}" --website-redirect "/vector/$i/$file" --acl public-read
    done
    echo "Redirected old artifact names"
  done

  echo "Add latest symlinks"
  find "$td" -maxdepth 1 -type f -print0 | while read -r -d $'\0' file ; do
    file=$(basename "$file")
    # vector-$version-amd64.deb -> vector-latest-amd64.deb
    echo -n "" | aws s3 cp - "s3://packages.timber.io/vector/latest/${file/$VERSION_EXACT/latest}" --website-redirect "/vector/latest/$file" --acl public-read
    # vector-$version-amd64.deb -> vector-amd64.deb
    echo -n "" | aws s3 cp - "s3://packages.timber.io/vector/latest/${file/$VERSION_EXACT-/}" --website-redirect "/vector/latest/$file" --acl public-read
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
    "https://packages.timber.io/vector/latest/vector-latest-x86_64-unknown-linux-gnu.tar.gz" \
    "$td/vector-$VERSION-x86_64-unknown-linux-gnu.tar.gz"

elif [[ "$CHANNEL" == "custom" ]]; then

  # Add custom files
  echo "Uploading all artifacts to s3://packages.timber.io/vector/custom"
  aws s3 cp "$td" "s3://packages.timber.io/vector/custom/$VERSION" --recursive --sse --acl public-read
  echo "Uploaded archives"

  # Verify that the files exist and can be downloaded
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  sleep "$VERIFY_TIMEOUT"
  verify_artifact \
    "https://packages.timber.io/vector/custom/$VERSION/vector-$VERSION-x86_64-unknown-linux-gnu.tar.gz" \
    "$td/vector-$VERSION-x86_64-unknown-linux-gnu.tar.gz"

fi

#
# Cleanup
#

rm -rf "$td"
rm -rf "$td_nightly"
