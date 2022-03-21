FROM docker.io/rust:1.59-bullseye@sha256:3f5fd9d366c2268c14e7f6a2ae0bfb3f3ef4037e89d428683f383ea8b1e9330b as builder
RUN apt-get update && apt-get -y install build-essential git clang cmake libclang-dev libsasl2-dev libstdc++-10-dev libssl-dev libxxhash-dev zlib1g-dev zlib1g
RUN git clone https://github.com/rui314/mold.git
RUN cd mold && git checkout v1.0.1 && make -j$(nproc) && make install
RUN rm -rf mold

# Smoke test
RUN ["/usr/local/bin/mold", "--version"]
