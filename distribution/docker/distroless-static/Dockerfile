FROM docker.io/alpine:3.24@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a AS builder

WORKDIR /vector

COPY vector-*-unknown-linux-musl*.tar.gz ./
RUN tar -xvf vector-0*-"$(cat /etc/apk/arch)"-unknown-linux-musl*.tar.gz --strip-components=2

RUN mkdir -p /var/lib/vector

# distroless doesn't use static tags
# hadolint ignore=DL3007
FROM gcr.io/distroless/static:latest@sha256:15f8637509792e05b97d83451b0b6ccfa4abd548f997e406ac1e72c87fe1fa78

# https://github.com/opencontainers/image-spec/blob/main/annotations.md
LABEL org.opencontainers.image.url="https://vector.dev"
LABEL org.opencontainers.image.source="https://github.com/vectordotdev/vector"
LABEL org.opencontainers.image.documentation="https://vector.dev/docs"

COPY --from=builder /vector/bin/* /usr/local/bin/
COPY --from=builder /vector/config/vector.yaml /usr/share/vector/examples/vector.yaml
COPY --from=builder /var/lib/vector /var/lib/vector

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/local/bin/vector"]
