ARG RUST_VERSION=1.85
FROM docker.io/rust:${RUST_VERSION}-slim-trixie

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    curl \
    g++ \
    git \
    golang-go \
    libclang1 \
    libsasl2-dev \
    libssl-dev \
    llvm \
    pkg-config \
    unzip \
    zlib1g-dev \
  && rm -rf /var/lib/apt/lists/*

COPY scripts/environment/ /scripts/environment/
RUN bash /scripts/environment/prepare.sh --modules=cargo-nextest,bindgen

COPY scripts/environment/install-protoc.sh /
COPY tests/data/ca/certs /certs
RUN bash /install-protoc.sh
