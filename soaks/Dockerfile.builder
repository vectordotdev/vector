FROM docker.io/rust:1.58-bullseye@sha256:d83bf5ea7b4c3d18c2f46d5f3d288bfca085c3e7ac57822e3b8e5a1ad22ccc1a as builder
RUN apt-get update && apt-get -y install build-essential git clang cmake libclang-dev libsasl2-dev libstdc++-10-dev libssl-dev libxxhash-dev zlib1g-dev zlib1g
RUN git clone https://github.com/rui314/mold.git
RUN cd mold && git checkout v1.0.1 && make -j$(nproc) && make install
RUN rm -rf mold

# Smoke test
RUN ["/usr/local/bin/mold", "--version"]
