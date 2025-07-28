#!/usr/bin/env bash

set -eux
set -o pipefail

BINSTALL_VERSION="v1.14.1"
BINSTALL_SHA256SUM_X86_64_LINUX="e1d1231720e6ed497a4b0f8881b08f5df9ce1a938fb3ae6f2444e95eb601fe99"
BINSTALL_SHA256SUM_AARCH64_LINUX="17d69bcc07a0e38c912e7f596ed71b1f5f59dc8980da59890c5bc86c07e8506a"
BINSTALL_SHA256SUM_ARMV7_LINUX="e4ba720023e02b071aa805ae62412e94741c1bb0e0a2bb2b35896fec3d140128"
BINSTALL_SHA256SUM_AARCH64_DARWIN="07d46d31fb68ac10b906c5d39d611ded7787966f4ed15c598cb6175b45a2b069"
BINSTALL_SHA256SUM_X86_64_DARWIN="3de381bdcca08c418dc790d2a283711894a0577c6e55bba0d4e6cb8b0378b36"

pushd "$(mktemp -d)"

base_url="https://github.com/cargo-bins/cargo-binstall/releases/download/${BINSTALL_VERSION}/cargo-binstall"

download() {
  curl --retry 3 --proto '=https' --tlsv1.2 -fsSL "$@"
}

os="$(uname -s)"
machine="$(uname -m)"

if [ "$os" = "Darwin" ]; then
  if [ "$machine" = "arm64" ]; then
    url="${base_url}-aarch64-apple-darwin.zip"
    download_sha256sum="${BINSTALL_SHA256SUM_AARCH64_DARWIN}"
  elif [ "$machine" = "x86_64" ]; then
    url="${base_url}-x86_64-apple-darwin.zip"
    download_sha256sum="${BINSTALL_SHA256SUM_X86_64_DARWIN}"
  else
    echo "Unsupported OS ${os} machine ${machine}"
    popd
    exit 1
  fi

  download -o output.zip "$url"
elif [ "$os" = "Linux" ]; then
  if [ "$machine" = "armv7l" ]; then
    target="armv7-unknown-linux-musleabihf"
    download_sha256sum="${BINSTALL_SHA256SUM_ARMV7_LINUX}"
  elif [ "$machine" = "aarch64" ]; then
    target="${machine}-unknown-linux-musl"
    download_sha256sum="${BINSTALL_SHA256SUM_AARCH64_LINUX}"
  elif [ "$machine" = "x86_64" ]; then
    target="${machine}-unknown-linux-musl"
    download_sha256sum="${BINSTALL_SHA256SUM_X86_64_LINUX}"
  else
    echo "Unsupported OS ${os} machine ${machine}"
    popd
    exit 1
  fi

  url="${base_url}-${target}.tgz"

  download -o output.tgz "$url"
# elif [ "${OS-}" = "Windows_NT" ]; then
#   target="${machine}-pc-windows-msvc"
#   url="${base_url}-${target}.zip"
#   download -o output.zip "$url"
else
    echo "Unsupported OS ${os}"
    popd
    exit 1
fi

echo "${download_sha256sum} $(echo output.*)" | sha256sum --check

case "$(echo output.*)" in
    *.zip) unzip output.* ;;
    *.tgz) tar -xvzf output.* ;;
    *) >&2 echo "output.* not found"; exit 1 ;;
esac

./cargo-binstall --self-install || ./cargo-binstall -y --force cargo-binstall
