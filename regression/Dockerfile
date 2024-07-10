#
# VECTOR BUILDER
#
FROM ghcr.io/vectordotdev/vector/soak-builder@sha256:c51a7091de2caebaa690e17f37dbfed4d4059dcdf5114a5596e8ca9b5ef494f3 as builder
WORKDIR /vector
COPY . .
RUN bash scripts/environment/install-protoc.sh
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/vector/target \
    /usr/local/bin/mold -run cargo build --bin vector --release && \
    cp target/release/vector .

#
# TARGET
#
FROM docker.io/debian:bookworm-slim@sha256:01bd742e2c269abf94e2fefb47b08b5b61c9a880b993417d23a1d0bd9fa60dc4
RUN apt-get update && apt-get dist-upgrade -y && apt-get -y --no-install-recommends install zlib1g ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /vector/vector /usr/bin/vector
RUN mkdir --parents --mode=0777 /var/lib/vector

# Smoke test
RUN ["/usr/bin/vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
