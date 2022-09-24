#! /usr/bin/env bash
set -o errexit -o verbose

TEMP=$(mktemp -d)

# Protoc. No guard because we want to override Ubuntu's old version in
# case it is already installed by a dependency.
PROTOC_VERSION=3.19.5
PROTOC_ZIP=protoc-${PROTOC_VERSION}-linux-x86_64.zip
curl -fsSL https://github.com/protocolbuffers/protobuf/releases/download/v$PROTOC_VERSION/$PROTOC_ZIP \
     --output "$TEMP/$PROTOC_ZIP"
unzip "$TEMP/$PROTOC_ZIP" bin/protoc -d "$TEMP"
chmod +x "$TEMP"/bin/protoc
mv --force --verbose "$TEMP"/bin/protoc /usr/bin/protoc
rm -fr "$TEMP"
