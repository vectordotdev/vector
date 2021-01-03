FROM alpine:3.12 AS builder

WORKDIR /vector

COPY vector-*-unknown-linux-musl*.tar.gz ./
RUN tar -xvf vector-0*-$(cat /etc/apk/arch)-unknown-linux-musl*.tar.gz --strip-components=2

FROM gcr.io/distroless/cc-debian10

COPY --from=builder /vector/bin/vector /usr/bin/vector
COPY --from=builder /vector/config /etc/vector
VOLUME /var/lib/vector/

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
