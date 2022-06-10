#
# VECTOR BUILDER
#
FROM ghcr.io/vectordotdev/vector/soak-builder@sha256:c51a7091de2caebaa690e17f37dbfed4d4059dcdf5114a5596e8ca9b5ef494f3 as builder
WORKDIR /vector
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/vector/target \
    /usr/local/bin/mold -run cargo build --bin vector --release && \
    cp target/release/vector .

#
# LADING
#
FROM ghcr.io/blt/lading@sha256:084ea90217d72d15174b6822890124f0c330a3aeef19195805dab625798323a3 as lading

#
# TARGET
#
FROM docker.io/debian:bullseye-slim@sha256:b0d53c872fd640c2af2608ba1e693cfc7dedea30abcd8f584b23d583ec6dadc7
RUN apt-get update && apt-get dist-upgrade -y && apt-get -y --no-install-recommends install zlib1g ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=lading /usr/bin/lading /usr/bin/lading
COPY --from=builder /vector/vector /usr/bin/vector
RUN mkdir --parents --mode=0777 /var/lib/vector

# Smoke test
RUN ["/usr/bin/lading", "--help"]
RUN ["/usr/bin/vector", "--version"]

ENTRYPOINT ["/usr/bin/lading"]
