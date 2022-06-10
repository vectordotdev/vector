ARG RUST_VERSION
FROM docker.io/rust:${RUST_VERSION}-slim-bullseye

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    curl \
    g++ \
    libclang1-9 \
    libsasl2-dev \
    libssl-dev \
    llvm-9 \
    pkg-config \
    zlib1g-dev \
  && rm -rf /var/lib/apt/lists/*

RUN curl -LsSf https://get.nexte.st/0.9/linux | tar zxf - -C /usr/local/bin
