ARG RUST_VERSION=1.57
ARG DEBIAN_VERSION=bullseye

#
# VECTOR BUILDER
#
FROM docker.io/rust:${RUST_VERSION}-${DEBIAN_VERSION} as builder
RUN apt-get update && apt-get -y install build-essential git clang cmake libclang-dev libsasl2-dev libstdc++-10-dev libssl-dev libxxhash-dev zlib1g-dev zlib1g
RUN git clone https://github.com/rui314/mold.git
RUN cd mold && git checkout v0.9.6 && make -j$(nproc) && make install
WORKDIR vector
COPY . .
RUN /usr/bin/mold -run cargo build --release --bin vector

#
# TARGET
#
FROM docker.io/debian:bullseye-slim
RUN apt-get update && apt-get -y install zlib1g
COPY --from=builder /vector/target/release/vector /usr/bin/vector
VOLUME /var/lib/vector/

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
