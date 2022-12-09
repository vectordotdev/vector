FROM docker.io/rust:1.62-bullseye@sha256:5709afe04a23c0a447b02077b3ab8ff3d9458e80e1f9898e40873df36a34981b as builder
RUN apt-get update && \
    apt-get dist-upgrade -y && \
    apt-get -y --no-install-recommends install build-essential git clang cmake libclang-dev \
    libsasl2-dev libstdc++-10-dev libssl-dev libxxhash-dev zlib1g-dev zlib1g && \
		rm -rf /var/lib/apt/lists/*

COPY scripts/environment/install-protoc.sh .
RUN bash ./install-protoc.sh

# Build mold, a fast linker
RUN git clone https://github.com/rui314/mold.git && cd mold && git checkout v1.2.1 && make -j"$(nproc)" && make install

# Smoke test
RUN ["/usr/local/bin/mold", "--version"]
