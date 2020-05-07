#!/bin/bash
set -euo pipefail

# package-msi.sh
#
# SUMMARY
#
#   Creates a .msi package for Windows.

set -x
rm -rf target/msi-x64
cp -R distribution/msi target/msi-x64
cp target/artifacts/vector-x86_64-pc-windows-msvc.zip target/msi-x64
pushd target/msi-x64
# shellcheck disable=SC2016
powershell '$progressPreference = "silentlyContinue"; Expand-Archive vector-x86_64-pc-windows-msvc.zip'
./build.sh
popd
cp target/msi-x64/vector.msi target/artifacts/vector-x64.msi
