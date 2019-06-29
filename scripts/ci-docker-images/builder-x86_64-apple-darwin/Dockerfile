# NOTE
#
# This docker image does not successfully compile Vector! It does build, but
# we were unable to produce a Vector binary using this Docker image.
# This image is an attempt to cross compile Vector to MacOS. We attempted to
# use osxcross and the osxcross.sh script successfully created binaries for
# the MacOS toolchain, but we failed to successfully compile Vector against
# this toolchain due to leveldb.

FROM ubuntu:18.04

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gcc \
    git \
    pkg-config \
    sed

RUN apt-get install -y --no-install-recommends \
    autotools-dev \
    automake \
    cmake \
    gcc \
    g++ \
    libbz2-dev \
    libmpc-dev \
    libmpfr-dev \
    libgmp-dev \
    libssl-dev \
    libxml2-dev \
    llvm-dev \
    uuid-dev \
    wget \
    zlib1g-dev

COPY osxcross.sh /
RUN bash /osxcross.sh
ENV PATH="/osxcross/target/bin:${PATH}"

# ADD https://raw.githubusercontent.com/rust-embedded/cross/master/docker/openssl.sh /
# gcc is 
# RUN sed -i s/CC=\${triple}gcc/CC=\${triple}clang++/g openssl.sh
# RUN bash /openssl.sh darwin64-x86_64-cc x86_64-apple-darwin18-

ENV CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=/osxcross/target/bin/x86_64-apple-darwin18-clang \
    OPENSSL_DIR=/openssl \
    OPENSSL_INCLUDE_DIR=/openssl/include \
    OPENSSL_LIB_DIR=/openssl/lib