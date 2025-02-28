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

apt-get update

# needed by onig_sys
apt-get install -y \
      libclang1-9 \
      llvm-9 \
      unzip

# aws-lc-rs dependencies
apt-get install -y \
 build-essential \
 libssl-dev

# Go installation is required for building aws-lc-rs
# https://github.com/aws/aws-lc/issues/2129
GO_VERSION="1.24.0"
GO_TAR_FILE="go${GO_VERSION}.linux-amd64.tar.gz"
wget https://go.dev/dl/${GO_TAR_FILE}
tar -C /usr/local -xzf ${GO_TAR_FILE}
rm ${GO_TAR_FILE}

# Set Go binary in PATH globally
echo "export PATH=\$PATH:/usr/local/go/bin" >> /etc/profile
. /etc/profile
