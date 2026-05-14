#!/bin/sh
set -o errexit

export DEBIAN_FRONTEND=noninteractive
export ACCEPT_EULA=Y

# Configure apt for speed and efficiency
cat > /etc/apt/apt.conf.d/90-vector-optimizations <<EOF
Acquire::Retries "5";
Acquire::Queue-Mode "host";
Acquire::Languages "none";
APT::Install-Recommends "false";
EOF


apt-get update
apt-get install -y \
  apt-transport-https \
  gnupg \
  wget \
  libclang1 \
  llvm \
  clang \
  unzip \
  libsasl2-dev

