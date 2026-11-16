FROM docker.io/debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2 AS builder

WORKDIR /vector

COPY vector_*.deb ./
RUN dpkg -i vector_*_"$(dpkg --print-architecture)".deb

RUN mkdir -p /var/lib/vector

# Stage libz at its arch-specific path so it can be copied into the distroless
# runtime. distroless/cc does not include zlib; vector (via rdkafka) links
# against it dynamically and we deliberately keep it as a runtime dependency.
RUN src=$(dpkg -L zlib1g | grep -E '/libz\.so\.1$') && \
    install -D "$(realpath "$src")" "/staging${src}"

# distroless doesn't use static tags
# hadolint ignore=DL3007
FROM gcr.io/distroless/cc-debian13:latest@sha256:809d04afa1be033c441f76bb0051878c342fb76d0ea236bc6dc94b655fa0fbd6

# https://github.com/opencontainers/image-spec/blob/main/annotations.md
LABEL org.opencontainers.image.url="https://vector.dev"
LABEL org.opencontainers.image.source="https://github.com/vectordotdev/vector"
LABEL org.opencontainers.image.documentation="https://vector.dev/docs"

COPY --from=builder /usr/bin/vector /usr/bin/vector
COPY --from=builder /usr/share/doc/vector /usr/share/doc/vector
COPY --from=builder /usr/share/vector /usr/share/vector
COPY --from=builder /etc/vector /etc/vector
COPY --from=builder /var/lib/vector /var/lib/vector
COPY --from=builder /staging/ /

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
