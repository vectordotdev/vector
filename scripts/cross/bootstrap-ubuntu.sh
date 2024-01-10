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
