#!/bin/sh
set -o errexit

echo 'Acquire::Retries "5";' > /etc/apt/apt.conf.d/80-retries

apt-get update
apt-get install -y \
  apt-transport-https \
  gnupg \
  wget

# needed by onig_sys
apt-get install -y \
      libclang1-14 \
      llvm-14 \
      unzip
