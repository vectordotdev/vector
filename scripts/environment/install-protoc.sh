#! /usr/bin/env bash
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

# shellcheck disable=SC2155
readonly TMP_DIR="$(mktemp -d -t "protoc_XXXX")"
trap 'rm -rf "${TMP_DIR?}"' EXIT

get_platform() {
  local os
  os=$(uname)
  if [[ "${os}" == "Darwin" ]]; then
    echo "osx"
  elif [[ "${os}" == "Linux" ]]; then
    echo "linux"
  else
    >&2 echo "unsupported os: ${os}" && exit 1
  fi
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

install_protoc() {
  local version=$1
  local install_path=$2

  local base_url="https://github.com/protocolbuffers/protobuf/releases/download"
  local url
  url="${base_url}/v${version}/protoc-${version}-$(get_platform)-$(get_arch).zip"
  local download_path="${TMP_DIR}/protoc.zip"

  echo "Downloading ${url}"
  curl -fsSL "${url}" -o "${download_path}"

  unzip -qq "${download_path}" -d "${TMP_DIR}"
  mv --force --verbose "${TMP_DIR}/bin/protoc" "${install_path}"
}

install_protoc "3.19.5" "${INSTALL_PATH}/protoc"
