#! /usr/bin/env bash
set -e -o verbose

export DEBIAN_FRONTEND=noninteractive

apt update --yes
apt upgrade --yes

# Deps
apt install --yes \
    wget \
    build-essential \
    pkg-config \
    libssl-dev \
    python3-pip \
    jq \
    shellcheck \
    software-properties-common \
    musl-tools \
    musl-dev \
    locales \
    apt-transport-https \
    ca-certificates \
    curl \
    gnupg-agent \
    ruby-bundler \
    nodejs \
    libsasl2-dev \
    tcl-dev \
    cmake \
    binutils-arm-linux-gnueabihf \
    gcc-arm-linux-gnueabihf \
    g++-arm-linux-gnueabihf \
    gnupg2
# Stupid, right? Sadly it works.
ln -s "/usr/bin/g++" "/usr/bin/musl-g++"

# Locales
locale-gen en_US.UTF-8
dpkg-reconfigure locales

# Yarn
curl -sS https://dl.yarnpkg.com/debian/pubkey.gpg | apt-key add -
echo "deb https://dl.yarnpkg.com/debian/ stable main" | tee /etc/apt/sources.list.d/yarn.list

# Docker
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | apt-key add -
add-apt-repository \
   "deb [arch=amd64] https://download.docker.com/linux/ubuntu \
   focal \
   stable"

# Install those new things
apt update --yes
apt install --yes yarn docker-ce docker-ce-cli containerd.io

# Remarshal is particular
pip3 install remarshal==0.11.2

# Cross toolchains
mkdir -p /git/richfelker/
git clone https://github.com/richfelker/musl-cross-make.git /git/richfelker/musl-cross-make
cd /git/richfelker/musl-cross-make
export NUM_CPUS=$(awk '/^processor/ { N++} END { print N }' /proc/cpuinfo)
make install -j${NUM_CPUS} TARGET=x86_64-linux-musl
make install -j${NUM_CPUS} TARGET=aarch64-linux-musl
make install -j${NUM_CPUS} TARGET=armv7l-linux-musleabihf
