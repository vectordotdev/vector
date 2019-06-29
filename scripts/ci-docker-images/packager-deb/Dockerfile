FROM ubuntu:18.04

# Install the git repository to ensure we get the latest version.
# This is important for determining the current Vector version number.
RUN apt-get update
RUN apt-get install -y software-properties-common
RUN add-apt-repository ppa:git-core/ppa

RUN apt-get update
RUN apt-get upgrade -y
RUN apt-get install -y \
  build-essential \
  curl \
  git \
  rename

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="$PATH:/root/.cargo/bin"
RUN cargo install cargo-deb