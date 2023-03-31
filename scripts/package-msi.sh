#!/bin/bash
set -euo pipefail

# package-msi.sh
#
# SUMMARY
#
#   Creates a .msi package for Windows.

set -x

ARCHIVE_VERSION="${VECTOR_VERSION:-"$(cargo vdev version)"}"

rm -rf target/msi-x64
cp -R distribution/msi target/msi-x64
cp target/artifacts/vector-"${ARCHIVE_VERSION}"-x86_64-pc-windows-msvc.zip target/msi-x64
pushd target/msi-x64
# shellcheck disable=SC2016
powershell '$progressPreference = "silentlyContinue"; Expand-Archive vector-'"$ARCHIVE_VERSION"'-x86_64-pc-windows-msvc.zip'
./build.sh "${ARCHIVE_VERSION}"
popd
cp target/msi-x64/vector.msi target/artifacts/vector-"${ARCHIVE_VERSION}"-x64.msi
