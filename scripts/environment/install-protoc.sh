#!/usr/bin/env bash
set -o errexit -o verbose

# A parameter can be optionally passed to this script to specify an alternative
# location to install protoc. Default is /usr/bin.
readonly INSTALL_PATH=${1:-"/usr/bin"}

if [[ -n $1 ]]
then
  mkdir -p "${INSTALL_PATH}"
fi

# Protoc. No guard because we want to override Ubuntu's old version in
# case it is already installed by a dependency.
#
# Basis of script copied from:
# https://github.com/paxosglobal/asdf-protoc/blob/46c2f9349b8420144b197cfd064a9677d21cfb0c/bin/install

# Pinned protoc release. Archives are verified against the SHA-256 digests in
# get_archive_sha256 before extraction. The archive names embed the version, so
# a version bump without new pins fails closed. Compute each digest from the
# official GitHub release assets, e.g.:
#   gh release download "v${PROTOC_VERSION}" -R protocolbuffers/protobuf \
#     -p "protoc-${PROTOC_VERSION}-*.zip" && shasum -a 256 protoc-*.zip
readonly PROTOC_VERSION="21.12"

get_archive_sha256() {
  case "$1" in
    protoc-21.12-linux-aarch_64.zip) echo "2dd17f75d66a682640b136e31848da9fb2eefe68d55303baf8b32617374f6711" ;;
    protoc-21.12-linux-x86_64.zip) echo "3a4c1e5f2516c639d3079b1586e703fc7bcfa2136d58bda24d1d54f949c315e8" ;;
    protoc-21.12-osx-aarch_64.zip) echo "96839af0caed64352442fc8236f4bdf7c1cd6efcfaa98fa5db37307a73fc7c70" ;;
    protoc-21.12-osx-x86_64.zip) echo "9448ff40278504a7ae5139bb70c962acc78c32d8fc54b4890a55c14c68b9d10a" ;;
    protoc-21.12-win64.zip) echo "71852a30cf62975358edfcbbff93086e8857a079c8e4d6904881aa968d65c7f9" ;;
    *) >&2 echo "unsupported platform: no pinned SHA-256 for ${1}" && return 1 ;;
  esac
}

# shellcheck disable=SC2155
readonly TMP_DIR="$(mktemp -d -t "protoc_XXXX")"
trap 'rm -rf "${TMP_DIR?}"' EXIT

get_platform() {
  local os
  os=$(uname)
  case "${os}" in
    Darwin) echo "osx" ;;
    Linux) echo "linux" ;;
    MINGW*|MSYS*|CYGWIN*) echo "win64" ;;
    *) >&2 echo "unsupported os: ${os}" && exit 1 ;;
  esac
}

get_arch() {
  local os
  local arch
  os=$(uname)
  arch=$(uname -m)
  # On ARM Macs, uname -m returns "arm64", but in protoc releases this architecture is called "aarch_64"
  if [[ "${os}" == "Darwin" && "${arch}" == "arm64" ]]; then
    echo "aarch_64"
  elif [[ "${os}" == "Linux" && "${arch}" == "aarch64" ]]; then
    echo "aarch_64"
  else
    echo "${arch}"
  fi
}

get_bin_name() {
  if [[ "$(get_platform)" == "win64" ]]; then
    echo "protoc.exe"
  else
    echo "protoc"
  fi
}

get_archive_name() {
  local platform=$1
  local arch=$2
  if [[ "${platform}" == "win64" ]]; then
    # Windows release assets are named without an explicit arch suffix.
    echo "protoc-${PROTOC_VERSION}-win64.zip"
  else
    echo "protoc-${PROTOC_VERSION}-${platform}-${arch}.zip"
  fi
}

# Prints the SHA-256 of a file using sha256sum (Linux, Git Bash) or shasum
# (macOS). Only executables on PATH are considered, never shell functions.
sha256_of() {
  local file=$1
  local output
  if type -P sha256sum >/dev/null; then
    output="$(command sha256sum "${file}")" || return 1
  elif type -P shasum >/dev/null; then
    output="$(command shasum -a 256 "${file}")" || return 1
  else
    >&2 echo "neither sha256sum nor shasum is available to verify ${file}"
    return 1
  fi
  echo "${output%% *}"
}

install_protoc() {
  local install_path=$1

  local platform
  local arch
  local archive
  local expected_sha256
  platform="$(get_platform)"
  arch="$(get_arch)"
  archive="$(get_archive_name "${platform}" "${arch}")"
  expected_sha256="$(get_archive_sha256 "${archive}")"

  local url="https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/${archive}"
  local download_path="${TMP_DIR}/${archive}"

  echo "Downloading ${url}"
  # Stay compatible with the curl shipped by Ubuntu 20.04 focal (7.68.x) used
  # in the ghcr.io/cross-rs/* base images: --retry-all-errors was only added
  # in curl 7.71.0, so rely on the transport-level retry that --retry covers.
  curl --retry 5 --retry-delay 10 -fsSL "${url}" -o "${download_path}"

  local actual_sha256
  actual_sha256="$(sha256_of "${download_path}")"
  if [[ "${actual_sha256}" != "${expected_sha256}" ]]; then
    >&2 echo "SHA-256 mismatch for ${archive}: expected ${expected_sha256}, got ${actual_sha256:-<none>}"
    exit 1
  fi

  unzip -qq "${download_path}" -d "${TMP_DIR}"
  mv -f -v "${TMP_DIR}/bin/$(get_bin_name)" "${install_path}"
}

install_protoc "${INSTALL_PATH}/$(get_bin_name)"
