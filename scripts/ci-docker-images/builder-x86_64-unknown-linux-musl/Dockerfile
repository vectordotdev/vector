FROM japaric/x86_64-unknown-linux-musl:latest

RUN apt-get update
RUN apt-get install -y software-properties-common
RUN add-apt-repository ppa:git-core/ppa

RUN apt-get update && \
  apt-get install -y --no-install-recommends \
  build-essential \
  curl \
  git \
  openssh-server \
  vim \
  zlib1g-dev

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="$PATH:/root/.cargo/bin"
ENV TARGET=x86_64-unknown-linux-musl
RUN rustup target add x86_64-unknown-linux-musl