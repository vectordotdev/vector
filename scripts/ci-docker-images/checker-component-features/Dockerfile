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
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain $(cat /tmp/rust-toolchain)
ENV PATH="$PATH:/root/.cargo/bin"
RUN rustup component add rustfmt

# Install `remarshal`
RUN pip3 install remarshal==0.11.2
