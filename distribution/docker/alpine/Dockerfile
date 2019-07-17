FROM alpine:latest

LABEL MAINTAINER="Vector Contributors <vector@timber.io>"

ENV VECTOR_VERSION latest

# Downloading Vector
ADD https://packages.timber.io/vector/${VECTOR_VERSION}/vector-${VECTOR_VERSION}-x86_64-unknown-linux-musl.tar.gz ./

# Installing packages
RUN mkdir vector && \
    mkdir /etc/vector && \
    tar -xzf vector-${VECTOR_VERSION}-x86_64-unknown-linux-musl.tar.gz --directory="vector" --strip-components=2 && \
    cp -a vector/bin/. /usr/local/bin && \
    cp -a vector/config/. /etc/vector && \
    rm vector-${VECTOR_VERSION}-x86_64-unknown-linux-musl.tar.gz && \
    rm -rf vector