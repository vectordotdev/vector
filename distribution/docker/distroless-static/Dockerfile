FROM docker.io/alpine:3.18 AS builder

WORKDIR /vector

COPY vector-*-unknown-linux-musl*.tar.gz ./
RUN tar -xvf vector-0*-"$(cat /etc/apk/arch)"-unknown-linux-musl*.tar.gz --strip-components=2

RUN mkdir -p /var/lib/vector

# distroless doesn't use static tags
# hadolint ignore=DL3007
FROM gcr.io/distroless/static:latest

COPY --from=builder /vector/bin/* /usr/local/bin/
COPY --from=builder /vector/config/vector.yaml /etc/vector/vector.yaml
COPY --from=builder /var/lib/vector /var/lib/vector

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/local/bin/vector"]
