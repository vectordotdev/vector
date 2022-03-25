FROM docker.io/rust:1.58-bullseye@sha256:e4979d36d5d30838126ea5ef05eb59c4c25ede7f064985e676feb21402d0661b as builder
RUN apt-get update && \
    apt-get dist-upgrade -y && \
    apt-get -y install build-essential git clang cmake libclang-dev \
    libsasl2-dev libstdc++-10-dev libssl-dev libxxhash-dev zlib1g-dev zlib1g

# Build mold, a fast linker
RUN git clone https://github.com/rui314/mold.git && cd mold && git checkout v1.1.1 && make -j$(nproc) && make install

# Smoke test
RUN ["/usr/local/bin/mold", "--version"]
