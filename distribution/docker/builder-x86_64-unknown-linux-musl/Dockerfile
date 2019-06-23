FROM japaric/x86_64-unknown-linux-musl:latest

# Install the git repository to ensure we get the latest version.
# This is important for determining the current Vector version number.
RUN apt-get update
RUN apt-get install -y software-properties-common
RUN add-apt-repository ppa:git-core/ppa

RUN echo "deb http://dk.archive.ubuntu.com/ubuntu/ trusty main universe
deb http://dk.archive.ubuntu.com/ubuntu/ trusty-updates main universe" | > /etc/apt/sources.list.d/fury.list

RUN apt-get update

# Note: We do not call `apt-get upgrade` because we do not want Vector to
#       depend on newer versions of libraries. This is just an assumption,
#       I could be completely wrong with how this works.

RUN apt-get install -y --no-install-recommends \
  build-essential \
  curl \
  g++-4.4 \
  gcc-4.4 \
  git \
  openssh-server \
  vim \
  libleveldb-dev \
  libsnappy-dev

RUN update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-4.4 50
RUN update-alternatives --install /usr/bin/g++ g++ /usr/bin/g++-4.4 50

RUN update-alternatives --install /usr/bin/cc cc /usr/bin/gcc 30
RUN update-alternatives --set cc /usr/bin/gcc

RUN update-alternatives --install /usr/bin/c++ c++ /usr/bin/g++ 30
RUN update-alternatives --set c++ /usr/bin/g++

# Note: We do not compile or install leveldb or rdkafka libraries because
#       those Rust creates automatically build and link the libraries for
#       this target. Unfortunately, this is not the case for other targets,
#       hence the reason we only support this target currently.

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="$PATH:/root/.cargo/bin"
ENV TARGET=x86_64-unknown-linux-musl
RUN rustup target add x86_64-unknown-linux-musl