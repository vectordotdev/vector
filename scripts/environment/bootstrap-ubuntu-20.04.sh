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
    locales \
    apt-transport-https \
    ca-certificates \
    curl \
    gnupg-agent \
    ruby-bundler \
    nodejs \
    libsasl2-dev \
    gnupg2 \
    wget \
    tcl-dev \
    cmake \
    crossbuild-essential-i386  \
    gnupg2

# Stupid, right? Sadly it works.
ln -s "/usr/bin/g++" "/usr/bin/musl-g++" || true

# We use Grease for parts of release.
TEMP=$(mktemp -d)
curl \
    -L https://github.com/timberio/grease/releases/download/v1.0.1/grease-1.0.1-linux-amd64.tar.gz \
    -o "${TEMP}/grease-1.0.1-linux-amd64.tar.gz"
tar \
    -xvf "${TEMP}/grease-1.0.1-linux-amd64.tar.gz" \
    -C "${TEMP}"
cp "${TEMP}/grease/bin/grease" /usr/bin/grease


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

