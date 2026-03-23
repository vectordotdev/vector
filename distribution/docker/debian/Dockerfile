FROM docker.io/debian:trixie-slim@sha256:1d3c811171a08a5adaa4a163fbafd96b61b87aa871bbc7aa15431ac275d3d430 AS builder

WORKDIR /vector

COPY vector_*.deb ./
RUN dpkg -i vector_*_"$(dpkg --print-architecture)".deb

RUN mkdir -p /var/lib/vector

FROM docker.io/debian:trixie-slim@sha256:1d3c811171a08a5adaa4a163fbafd96b61b87aa871bbc7aa15431ac275d3d430

# https://github.com/opencontainers/image-spec/blob/main/annotations.md
LABEL org.opencontainers.image.url="https://vector.dev"
LABEL org.opencontainers.image.source="https://github.com/vectordotdev/vector"
LABEL org.opencontainers.image.documentation="https://vector.dev/docs"

# we want the latest versions of these
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata systemd libsasl2-2 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/bin/vector /usr/bin/vector
COPY --from=builder /usr/share/vector /usr/share/vector
COPY --from=builder /usr/share/doc/vector /usr/share/doc/vector
COPY --from=builder /etc/vector /etc/vector
COPY --from=builder /var/lib/vector /var/lib/vector

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
