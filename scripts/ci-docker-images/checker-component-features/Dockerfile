FROM ubuntu:18.04

RUN apt-get update && \
  apt-get install -y \
  build-essential \
  curl \
  git \
  jq \
  libssl-dev \
  pkg-config \
  python3-pip

# Install Rust
COPY rust-toolchain /tmp/
ARG RUST_PREFIX="/opt/rust"
ENV RUSTUP_HOME="$RUST_PREFIX/rustup"
ENV CARGO_HOME="$RUST_PREFIX/cargo"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$(cat /tmp/rust-toolchain)"
ENV PATH="$RUSTUP_HOME/bin:$CARGO_HOME/bin:$PATH"
RUN rustup component add rustfmt

# Install `remarshal`
RUN pip3 install remarshal==0.11.2
