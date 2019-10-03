FROM alpine:3.10.1 AS builder

# The `x.y.z` version of vector to fetch from packages.timber.io
ENV VERSION nightly/latest

WORKDIR /vector

RUN apk add wget

RUN wget https://packages.timber.io/vector/${VERSION}/vector-x86_64-unknown-linux-musl.tar.gz

RUN tar -xvf vector-x86_64-unknown-linux-musl.tar.gz
RUN mv vector-x86_64-unknown-linux-musl/bin bin

FROM alpine:3.10.1

COPY --from=builder /vector/bin/* /usr/local/bin/

ENTRYPOINT ["/usr/local/bin/vector"]
