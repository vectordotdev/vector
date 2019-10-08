FROM alpine:3.10.1 AS builder

COPY vector-x86_64-unknown-linux-musl.tar.gz .
RUN tar -xvf vector-x86_64-unknown-linux-musl.tar.gz --strip-components=2

FROM alpine:3.10.1

COPY /vector/bin/* /usr/local/bin/

ENTRYPOINT ["/usr/local/bin/vector"]
