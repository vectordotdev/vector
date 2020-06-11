FROM ruby:2.7

RUN apt-get update && \
  apt-get install -y \
  build-essential \
  curl \
  cmake \
  git

# Install Rust
COPY rust-toolchain /tmp/
ARG RUST_PREFIX="/opt/rust"
ENV RUSTUP_HOME="$RUST_PREFIX/rustup"
ENV CARGO_HOME="$RUST_PREFIX/cargo"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$(cat /tmp/rust-toolchain)"
ENV PATH="$RUSTUP_HOME/bin:$CARGO_HOME/bin:$PATH"
RUN rustup component add clippy rustfmt

# Install Ruby dependencies
ENV LC_ALL C.UTF-8
RUN gem install bundler -v '~> 2.1.4'
COPY scripts/Gemfile Gemfile
COPY scripts/Gemfile.lock Gemfile.lock
RUN bundle install
RUN rm Gemfile
