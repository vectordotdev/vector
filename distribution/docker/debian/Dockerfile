FROM debian:buster AS builder

# The `x.y.z` version of vector to fetch from packages.timber.io
ARG version=nightly/latest

WORKDIR /vector

RUN apt-get update && apt-get install -y wget

RUN wget https://packages.timber.io/vector/${version}/vector-amd64.deb

FROM debian:buster-slim

COPY --from=builder /vector/vector-amd64.deb /vector-amd64.deb
RUN dpkg -i vector-amd64.deb
RUN rm /vector-amd64.deb

ENTRYPOINT ["/usr/bin/vector"]
