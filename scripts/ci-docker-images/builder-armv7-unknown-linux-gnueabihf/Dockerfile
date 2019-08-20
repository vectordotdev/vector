FROM japaric/armv7-unknown-linux-gnueabihf:latest

RUN apt-get update
RUN apt-get install -y software-properties-common python-software-properties
RUN apt-get update
RUN add-apt-repository -y ppa:git-core/ppa
RUN apt-get update
RUN apt-get install -y --no-install-recommends \
    curl \
    git \
    openssh-server \
    vim \
    zlib1g-dev

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="$PATH:/root/.cargo/bin"
ENV TARGET=armv7-unknown-linux-gnueabihf
RUN rustup target add armv7-unknown-linux-gnueabihf