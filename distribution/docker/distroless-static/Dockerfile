FROM debian:buster-slim AS builder

WORKDIR /vector

COPY vector-x86_64-unknown-linux-musl*.tar.gz ./
RUN tar -xvf vector-x86_64-unknown-linux-musl*.tar.gz --strip-components=2

FROM gcr.io/distroless/static

COPY --from=builder /vector/bin/* /usr/local/bin/
COPY --from=builder /vector/config/vector.toml /etc/vector/vector.toml
VOLUME /var/lib/vector/

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/local/bin/vector"]
