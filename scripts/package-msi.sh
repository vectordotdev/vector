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

# Building the MSI package requires the version to be purely numerical (eg 0.0.0),
# which is not the case if with custom build workflow.
# This specifically works around the following issue:
#     C:\a\vector\vector\target\msi-x64\vector.wxs(6) : error CNDL0108 : The Product/@Version attribute's value, '0.29.0.custom.a28ecdc', is not a valid version.
#     Legal version values should look like 'x.x.x.x' where x is an integer from 0 to 65534.
# , by  changing "0.29.0.custom.a28ecdc" -> "0.29.0".
CHANNEL="${CHANNEL:-"$(cargo vdev release channel)"}"

if [[ "$CHANNEL" == "custom" ]]; then
    PACKAGE_VERSION="${ARCHIVE_VERSION%.custom*}"
else
    PACKAGE_VERSION="${ARCHIVE_VERSION}"
fi

./build.sh "${ARCHIVE_VERSION}" "${PACKAGE_VERSION}"
popd
cp target/msi-x64/vector.msi target/artifacts/vector-"${ARCHIVE_VERSION}"-x64.msi
