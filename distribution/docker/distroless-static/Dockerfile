FROM docker.io/alpine:3.18 AS builder

WORKDIR /vector

COPY vector-*-unknown-linux-musl*.tar.gz ./
RUN tar -xvf vector-0*-"$(cat /etc/apk/arch)"-unknown-linux-musl*.tar.gz --strip-components=2

RUN mkdir -p /var/lib/vector

FROM gcr.io/distroless/static

COPY --from=builder /vector/bin/* /usr/local/bin/
COPY --from=builder /vector/config/vector.toml /etc/vector/vector.toml
COPY --from=builder /var/lib/vector /var/lib/vector

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/local/bin/vector"]
