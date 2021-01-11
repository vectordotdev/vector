FROM alpine:3.12 AS builder

WORKDIR /vector

COPY vector-*-unknown-linux-musl*.tar.gz ./
RUN tar -xvf vector-0*-$(cat /etc/apk/arch)-unknown-linux-musl*.tar.gz --strip-components=2

FROM alpine:3.12
RUN apk update && apk add ca-certificates tzdata && rm -rf /var/cache/apk/*

COPY --from=builder /vector/bin/* /usr/local/bin/
COPY --from=builder /vector/config/vector.toml /etc/vector/vector.toml
VOLUME /var/lib/vector/

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/local/bin/vector"]
