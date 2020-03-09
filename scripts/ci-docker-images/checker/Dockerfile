FROM ruby:2.7

RUN apt-get update && \
  apt-get install -y \
  build-essential \
  curl \
  git

# Install Rust
COPY rust-toolchain /tmp/
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain $(cat /tmp/rust-toolchain)
ENV PATH="$PATH:/root/.cargo/bin"
RUN rustup component add rustfmt

# Install Ruby dependencies
ENV LC_ALL C.UTF-8
RUN gem install bundler -v '~> 2.1.4'
COPY scripts/Gemfile Gemfile
COPY scripts/Gemfile.lock Gemfile.lock
RUN bundle install
RUN rm Gemfile
