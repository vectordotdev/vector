#! /usr/bin/env bash
set -e -o verbose

if [ -n "$RUSTFLAGS" ]
then
  # shellcheck disable=SC2016
  echo '$RUSTFLAGS MUST NOT be set in CI configs as it overrides settings in `.cargo/config.toml`.'
  exit 1
fi

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
    docker-compose \
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
    -L https://github.com/cue-lang/cue/releases/download/v0.4.2/cue_v0.4.2_linux_amd64.tar.gz \
    -o "${TEMP}/cue_v0.4.2_linux_amd64.tar.gz"
tar \
    -xvf "${TEMP}/cue_v0.4.2_linux_amd64.tar.gz" \
    -C "${TEMP}"
cp "${TEMP}/cue" /usr/bin/cue

# Grease
# Grease is used for the `make release-github` task.
TEMP=$(mktemp -d)
curl \
    -L https://github.com/vectordotdev/grease/releases/download/v1.0.1/grease-1.0.1-linux-amd64.tar.gz \
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
# Also, force the proto-build crate to avoid building the vendored protoc.
if [ -n "${CI-}" ] ; then
    echo "${HOME}/.cargo/bin" >> "${GITHUB_PATH}"
    # we often run into OOM issues in CI due to the low memory vs. CPU ratio on c5 instances
    echo "CARGO_BUILD_JOBS=$(($(nproc) /2))" >> "${GITHUB_ENV}"
    echo PROTOC_NO_VENDOR=1 >> "${GITHUB_ENV}"
else
    echo "export PATH=\"$HOME/.cargo/bin:\$PATH\"" >> "${HOME}/.bash_profile"
    echo "export PROTOC_NO_VENDOR=1" >> "${HOME}/.bash_profile"
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

# Protoc. No guard because we want to override Ubuntu's old version in
# case it is already installed by a dependency.
PROTOC_VERSION=3.19.4
PROTOC_ZIP=protoc-${PROTOC_VERSION}-linux-x86_64.zip
curl -fsSL https://github.com/protocolbuffers/protobuf/releases/download/v$PROTOC_VERSION/$PROTOC_ZIP \
     --output "$TEMP/$PROTOC_ZIP"
unzip "$TEMP/$PROTOC_ZIP" bin/protoc -d "$TEMP"
chmod +x "$TEMP"/bin/protoc
mv --force --verbose "$TEMP"/bin/protoc /usr/bin/protoc

# Apt cleanup
apt clean

# Set up the default "deny all warnings" build flags
CARGO_OVERRIDE_DIR="${HOME}/.cargo"
CARGO_OVERRIDE_CONF="${CARGO_OVERRIDE_DIR}/config.toml"
cat <<EOF >>"$CARGO_OVERRIDE_CONF"
[target.'cfg(linux)']
rustflags = [ "-D", "warnings" ]
EOF

# Install mold, because the system linker wastes a bunch of time.
#
# Notably, we don't install/configure it when we're going to do anything with `cross`, as `cross` takes the Cargo
# configuration from the host system and ships it over...  which isn't good when we're overriding the `rustc-wrapper`
# and all of that.
if [ -n "${DISABLE_MOLD-}" ] ; then
    # We explicitly put `mold-wrapper.so` right beside `mold` itself because it's hard-coded to look in the same directory
    # first when trying to load the shared object, so we can dodge having to care about the "right" lib folder to put it in.
    TEMP=$(mktemp -d)
    MOLD_VERSION=1.2.1
    MOLD_TARGET=mold-${MOLD_VERSION}-x86_64-linux
    curl -fsSL "https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/${MOLD_TARGET}.tar.gz" \
        --output "$TEMP/${MOLD_TARGET}.tar.gz"
    tar \
        -xvf "${TEMP}/${MOLD_TARGET}.tar.gz" \
        -C "${TEMP}"
    cp "${TEMP}/${MOLD_TARGET}/bin/mold" /usr/bin/mold
    cp "${TEMP}/${MOLD_TARGET}/lib/mold/mold-wrapper.so" /usr/bin/mold-wrapper.so

    # Create our rustc wrapper script that we'll use to actually invoke `rustc` such that `mold` will wrap it and intercept
    # anything linking calls to use `mold` instead of `ld`, etc.
    CARGO_BIN_DIR="${CARGO_OVERRIDE_DIR}/bin"
    mkdir -p "$CARGO_BIN_DIR"

    RUSTC_WRAPPER="${CARGO_BIN_DIR}/wrap-rustc"
    cat <<EOF >"$RUSTC_WRAPPER"
#!/bin/sh
set -x
exec mold -run "\$@"
EOF
    chmod +x "$RUSTC_WRAPPER"

    # Now configure Cargo to use our rustc wrapper script.
    cat <<EOF >>"$CARGO_OVERRIDE_CONF"
[build]
rustc-wrapper = "$RUSTC_WRAPPER"
EOF
fi
