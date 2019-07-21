FROM alpine:3.10.1

RUN apk add git make cmake g++ gcc rust cargo snappy-dev protobuf-dev openssl-dev bash

ENV PROTOC /usr/bin/protoc

# https://github.com/sfackler/rust-openssl/issues/604
ENV OPENSSL_STATIC 1
ENV OPENSSL_INCLUDE_DIR /usr/include
ENV OPENSSL_LIB_DIR /usr/lib

ENV LIBZ_SYS_STATIC 1

ENV TARGET=x86_64-unknown-linux-musl