# It's important that we keep this and not extend our own base image.
# When we extend our own base image we get a number of dependency errors
# on CentOS:9.
FROM japaric/x86_64-unknown-linux-gnu:latest

# Install the git repository to ensure we get the latest version.
# This is important for determining the current Vector version number.
RUN apt-get update && \
  apt-get install -y \
  software-properties-common \
  python-software-properties

RUN add-apt-repository -y ppa:git-core/ppa

RUN apt-get update && \
  apt-get install -y --no-install-recommends \
  curl \
  git \
  openssh-server \
  vim

# Note: We do not compile or install leveldb or rdkafka libraries because
#       those Rust creates automatically build and link the libraries for
#       this target. Unfortunately, this is not the case for other targets,
#       hence the reason we only support this target currently.

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="$PATH:/root/.cargo/bin"
RUN cargo install cargo-deb

ENV TARGET=x86_64-unknown-linux-gnu