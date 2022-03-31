ARG RUST_VERSION
ARG DEBIAN_RELEASE=bullseye
# Features required for both Agent and Aggregator Helm chart configurations
ARG FEATURES=api,api-client,sources-datadog_agent,sources-fluent,sources-host_metrics,sources-internal_metrics,sources-kubernetes_logs,sources-logstash,sources-splunk_hec,sources-statsd,sources-syslog,sources-vector,sinks-console,sinks-prometheus,sinks-vector

#
# VECTOR BUILDER
#
FROM docker.io/rust:${RUST_VERSION}-${DEBIAN_RELEASE} as builder
RUN apt-get update && apt-get -y install build-essential git clang cmake libclang-dev libsasl2-dev libstdc++-10-dev libssl-dev libxxhash-dev zlib1g-dev zlib1g
RUN git clone https://github.com/rui314/mold.git
RUN cd mold && git checkout v1.1.1 && make -j$(nproc) && make install && rm -rf mold

WORKDIR /vector
COPY . .
ARG FEATURES
RUN --mount=type=cache,target=/vector/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    /usr/local/bin/mold -run cargo build --bin vector \
    --no-default-features --features $FEATURES && \
    cp target/debug/vector .

#
# TARGET
#
FROM debian:${DEBIAN_RELEASE}-slim
RUN apt-get update && apt-get -y install zlib1g
COPY --from=builder /vector/vector /usr/bin/vector
VOLUME /var/lib/vector/

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]

