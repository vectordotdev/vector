#!/bin/sh
set -o errexit

echo 'Acquire::Retries "5";' > /etc/apt/apt.conf.d/80-retries

apt-get update
apt-get install -y \
  apt-transport-https \
  gnupg \
  wget

# we need LLVM >= 3.9 for onig_sys/bindgen

cat <<-EOF > /etc/apt/sources.list.d/llvm.list
deb http://apt.llvm.org/xenial/ llvm-toolchain-xenial-9 main
deb-src http://apt.llvm.org/xenial/ llvm-toolchain-xenial-9 main
EOF

wget -O - https://apt.llvm.org/llvm-snapshot.gpg.key| apt-key add -

# onig_sys and aws-lc-rs dependencies
apt-get update
apt-get install -y \
  gcc-arm-linux-gnueabihf \
  g++-arm-linux-gnueabihf \
  gcc-aarch64-linux-gnu \
  g++-aarch64-linux-gnu \
  libc6-dev-armhf-cross \
  libc6-dev-arm64-cross \
  clang \
  cmake \
  libssl-dev \
  libclang-dev \
  libsasl2-dev \
  llvm \
  unzip

# Required by the `rdkafka-sys` Rust dependency
ZLIB_VERSION=1.3.1
wget https://www.zlib.net/zlib-${ZLIB_VERSION}.tar.gz
tar xzvf  zlib-${ZLIB_VERSION}.tar.gz
cd zlib-${ZLIB_VERSION}
./configure
make
make install

# Go installation is required for building aws-lc-rs
# https://github.com/aws/aws-lc/issues/2129
GO_VERSION="1.24.7"
SHA="da18191ddb7db8a9339816f3e2b54bdded8047cdc2a5d67059478f8d1595c43f"
GO_TAR_FILE="go${GO_VERSION}.linux-amd64.tar.gz"
wget https://go.dev/dl/${GO_TAR_FILE}
echo "${SHA} ${GO_TAR_FILE}" | sha256sum -c -
tar -C /usr/local -xzf ${GO_TAR_FILE}
rm ${GO_TAR_FILE}
ln -s /usr/local/go/bin/go /usr/local/bin/go

/scripts/environment/prepare.sh --modules=rustup,bindgen
ln -s "$(dirname "$(which cargo)")/"* /usr/local/bin/
