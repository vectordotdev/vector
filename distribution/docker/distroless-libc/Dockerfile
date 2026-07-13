FROM docker.io/debian:trixie-slim@sha256:28de0877c2189802884ccd20f15ee41c203573bd87bb6b883f5f46362d24c5c2 AS builder

WORKDIR /vector

# Shared libs to copy into distroless/cc (not shipped there).
# hadolint ignore=DL3008
RUN apt-get update \
    && apt-get install -y --no-install-recommends libodbc2 \
    && rm -rf /var/lib/apt/lists/*

COPY vector_*.deb ./
RUN dpkg-deb -x vector_*_"$(dpkg --print-architecture)".deb /

RUN mkdir -p /var/lib/vector

RUN src=$(dpkg -L zlib1g | grep -E '/libz\.so\.1$') \
    && install -D "$(realpath "$src")" "/staging${src}" \
    && src=$(dpkg -L libodbc2 | grep -E '/libodbc\.so\.2$') \
    && install -D "$(realpath "$src")" "/staging${src}" \
    && src=$(dpkg -L libltdl7 | grep -E '/libltdl\.so\.7$') \
    && install -D "$(realpath "$src")" "/staging${src}"

# distroless doesn't use static tags
# hadolint ignore=DL3007
FROM gcr.io/distroless/cc-debian12:latest@sha256:d703b626ba455c4e6c6fbe5f36e6f427c85d51445598d564652a2f334179f96e

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
