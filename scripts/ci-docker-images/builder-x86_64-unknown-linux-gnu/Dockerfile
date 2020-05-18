# The official Rust image based on Debian 8 Jessie.
FROM rust:jessie
# This image was not updated since 2018, so Cargo version inside it does not
# support editions, which makes it impossible to install `cargo-deb`.
RUN rustup update stable
RUN rustup run stable cargo install cargo-deb --version '^1.24.0'
RUN apt-get update && apt-get install -y cmake
RUN cd /tmp && \
  git clone https://github.com/github/cmark-gfm && \
  cd cmark-gfm && \
  git checkout 0.29.0.gfm.0 && \
  make install INSTALL_PREFIX=/usr && \
  cd .. && \
  rm -rf cmark-gfm
