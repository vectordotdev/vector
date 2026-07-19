FROM docker.io/debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd AS builder

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
FROM gcr.io/distroless/cc-debian12:latest@sha256:7ee09f36862efbdbf70422db263e411c2618409ca46faa555bd5b636155307df

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
