#! /usr/bin/env bash
set -e -o verbose

export DEBIAN_FRONTEND=noninteractive
export ACCEPT_EULA=Y

echo 'APT::Acquire::Retries "5";' > /etc/apt/apt.conf.d/80-retries

apt update --yes

apt install --yes \
  software-properties-common \
  apt-utils \
  apt-transport-https

apt upgrade --yes

# Deps
apt install --yes \
    awscli \
    build-essential \
    ca-certificates \
    cmake \
    cmark-gfm \
    curl \
    gawk \
    gnupg2 \
    gnupg-agent \
    gnuplot \
    jq \
    libclang-dev \
    libsasl2-dev \
    libssl-dev \
    llvm \
    locales \
    nodejs \
    npm \
    pkg-config \
    python3-pip \
    rename \
    rpm \
    ruby-bundler \
    shellcheck \
    sudo \
    wget \
    yarn

# Cue
TEMP=$(mktemp -d)
curl \
    -L https://github.com/cue-lang/cue/releases/download/v0.4.0/cue_v0.4.0_linux_amd64.tar.gz \
    -o "${TEMP}/cue_v0.4.0_linux_amd64.tar.gz"
tar \
    -xvf "${TEMP}/cue_v0.4.0_linux_amd64.tar.gz" \
    -C "${TEMP}"
cp "${TEMP}/cue" /usr/bin/cue

# Grease
# Grease is used for the `make release-github` task.
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

if ! command -v rustup ; then
  # Rust/Cargo should already be installed on both GH Actions-provided Ubuntu 20.04 images _and_
  # by our own Ubuntu 20.04 images
  curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal
fi

# Rust/Cargo should already be installed on both GH Actions-provided Ubuntu 20.04 images _and_
# by our own Ubuntu 20.04 images, so this is really just make sure the path is configured.
if [ -n "${CI-}" ] ; then
    echo "${HOME}/.cargo/bin" >> "${GITHUB_PATH}"
    # we often run into OOM issues in CI due to the low memory vs. CPU ratio on c5 instances
    echo "CARGO_BUILD_JOBS=$(($(nproc) /2))" >> "${GITHUB_ENV}"
else
    echo "export PATH=\"$HOME/.cargo/bin:\$PATH\"" >> "${HOME}/.bash_profile"
fi

# Docker.
if ! [ -x "$(command -v docker)" ]; then
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | apt-key add -
    add-apt-repository \
        "deb [arch=$(dpkg --print-architecture)] https://download.docker.com/linux/ubuntu \
        xenial \
        stable"
    # Install those new things
    apt update --yes
    apt install --yes docker-ce docker-ce-cli containerd.io

    # ubuntu user doesn't exist in scripts/environment/Dockerfile which runs this
    usermod --append --groups docker ubuntu || true
fi

# Apt cleanup
apt clean
