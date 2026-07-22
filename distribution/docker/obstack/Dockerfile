# syntax=docker/dockerfile:1.7
FROM rust:1.97-bookworm AS build
WORKDIR /src
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler openssh-client \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p -m 0700 /root/.ssh \
    && ssh-keyscan -t ed25519,rsa github.com >> /root/.ssh/known_hosts
COPY . .
# obstack-wire (sinks-iggy feature) is a private git dependency fetched over
# SSH; forward the host's ssh-agent via buildx --ssh default.
RUN --mount=type=ssh cargo build --locked --release --no-default-features \
      --features sources-datadog_agent,sources-opentelemetry,sinks-iggy \
      --bin vector

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 vector \
    && mkdir -p /etc/vector /var/lib/vector \
    && chown -R vector:vector /var/lib/vector
COPY --from=build /src/target/release/vector /usr/local/bin/vector
COPY --chmod=0755 distribution/docker/obstack/entrypoint.sh /usr/local/bin/obstack-vector-entrypoint
USER vector
VOLUME ["/var/lib/vector"]
EXPOSE 4317 4318 8080
ENTRYPOINT ["/usr/local/bin/obstack-vector-entrypoint"]
CMD ["/usr/local/bin/vector", "--config", "/etc/vector/vector.yaml"]
