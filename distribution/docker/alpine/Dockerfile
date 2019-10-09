FROM alpine:3.10.1 AS builder

WORKDIR /vector

COPY vector-x86_64-unknown-linux-musl.tar.gz .
RUN tar -xvf vector-x86_64-unknown-linux-musl.tar.gz --strip-components=2

FROM alpine:3.10.1

COPY --from=builder /vector/bin/* /usr/local/bin/
COPY --from=builder /vector/config/vector.toml /etc/vector/vector.toml

ENTRYPOINT ["/usr/local/bin/vector"]
