# LLVM target triple for Linux and MUSL. Full list of supported triples
# can be found here:
# https://forge.rust-lang.org/release/platform-support.html.
# Example: "armv7-unknown-linux-musleabihf".
ARG TARGET="x86_64-unknown-linux-gnu"

# Locations for produced tools and libraries.
ARG RUST_PREFIX=/opt/rust
ARG CLANG_PREFIX=/usr
ARG LIBS_PREFIX=/opt/libs

# The actual builder.
FROM ubuntu:14.04

# Install Rust using rustup.
RUN apt-get update && apt-get install -y curl
ARG TARGET
ARG RUST_PREFIX
ENV RUSTUP_HOME=$RUST_PREFIX/rustup
ENV CARGO_HOME=$RUST_PREFIX/cargo
COPY rust-toolchain /tmp/
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
  sh -s -- -y --profile minimal --default-toolchain $(cat /tmp/rust-toolchain) --target $TARGET
ENV PATH="$RUSTUP_HOME/bin:$CARGO_HOME/bin:$PATH"

# RUN apt-get update && apt-get install -y llvm clang lld

# Essential tools needed for compiling native dependencies
RUN apt-get update && apt-get install -y --install-recommends build-essential cmake git

# Add dependencies for running tests
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y tzdata ca-certificates

ENV TARGET="$TARGET"

# Install `cargo-deb from Git until
# https://github.com/mmstick/cargo-deb/pull/112 is published on crates.io
# RUN cargo install --git https://github.com/mmstick/cargo-deb --rev cc879930c06fa22e99f2839bad620ac0d0da879e cargo-deb
# RUN apt-get install -y rename cmark-gfm
