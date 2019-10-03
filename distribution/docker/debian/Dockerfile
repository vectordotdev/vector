FROM debian:buster AS builder

# The `x.y.z` version of vector to fetch from packages.timber.io
ENV VERSION nightly/latest

WORKDIR /vector

RUN apt-get update && apt-get install -y wget

# TODO: Use https instead of http
RUN wget https://packages.timber.io/vector/${VERSION}/vector-x86_64-unknown-linux-gnu.tar.gz

RUN tar -xvf vector-x86_64-unknown-linux-gnu.tar.gz
RUN mv vector-x86_64-unknown-linux-gnu/bin bin

FROM debian:buster-slim

COPY --from=builder /vector/bin/* /usr/local/bin/

ENTRYPOINT ["/usr/local/bin/vector"]
