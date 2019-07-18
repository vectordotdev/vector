FROM alpine:latest AS builder

LABEL MAINTAINER="Vector Contributors <vector@timber.io>"

ENV VECTOR_VERSION latest

WORKDIR /scripts

# Downloading Vector
ADD https://packages.timber.io/vector/${VECTOR_VERSION}/vector-${VECTOR_VERSION}-x86_64-unknown-linux-musl.tar.gz ./

# Installing packages
RUN mkdir vector && \
    mkdir /etc/vector && \
    tar -xzf vector-${VECTOR_VERSION}-x86_64-unknown-linux-musl.tar.gz --directory="vector" --strip-components=2 && \
    cp -a vector/config/. /etc/vector

FROM alpine:latest

COPY --from=builder /scripts/vector/bin/* /usr/local/bin/
COPY --from=builder /etc/vector /etc