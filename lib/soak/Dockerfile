#
# BUILDER
#
FROM docker.io/rust:1.58-buster as builder
WORKDIR vector
ARG VECTOR_FEATURES
# RUN apt-get -y update && apt-get -y install build-essential cmake libclang-dev libsasl2-dev
COPY . .
RUN cargo build --release --package soak --bin observer --no-default-features

#
# TARGET
#
FROM gcr.io/distroless/cc-debian11
COPY --from=builder /vector/target/release/observer /usr/bin/observer

# Smoke test
RUN ["observer", "--help"]

ENTRYPOINT ["/usr/bin/observer"]
