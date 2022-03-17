#
# VECTOR BUILDER
#
FROM ghcr.io/vectordotdev/vector/soak-builder@sha256:c51a7091de2caebaa690e17f37dbfed4d4059dcdf5114a5596e8ca9b5ef494f3 as builder
# Smoke test, ensure image has the background bits we need before going through
# an expensive release build.
RUN ["/usr/local/bin/lading", "--help"]
RUN ["/usr/local/bin/mold", "--version"]
WORKDIR vector
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/vector/target \
    /usr/local/bin/mold -run cargo build --bin vector --release && \
    cp target/release/vector .

#
# LADING
#
FROM ghcr.io/blt/lading@sha256:bb722f54b03c35da0fcd7ce37569f42eda40dca84040b7b0686bd802651023da as lading

#
# TARGET
#
FROM docker.io/debian:bullseye-slim@sha256:b0d53c872fd640c2af2608ba1e693cfc7dedea30abcd8f584b23d583ec6dadc7
RUN apt-get update && apt-get dist-upgrade -y && apt-get -y install zlib1g ca-certificates
COPY --from=lading /usr/bin/lading /usr/bin/lading
COPY --from=builder /vector/vector /usr/bin/vector
VOLUME /var/lib/vector/

# Smoke test
RUN ["/usr/bin/lading", "--help"]
RUN ["/usr/bin/vector", "--version"]

ENTRYPOINT ["/usr/bin/lading"]
