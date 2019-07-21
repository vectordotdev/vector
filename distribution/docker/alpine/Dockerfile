FROM alpine:3.10.1 AS builder

RUN apk add git make cmake g++ gcc rust cargo snappy-dev protobuf-dev openssl-dev bash
RUN git clone https://github.com/timberio/vector && cd vector && git checkout d6f3a1a4c2f8da71b950725f7bb164f526c12386

WORKDIR /vector

ENV PROTOC /usr/bin/protoc

# https://github.com/sfackler/rust-openssl/issues/604
ENV OPENSSL_STATIC 1
ENV OPENSSL_INCLUDE_DIR /usr/include
ENV OPENSSL_LIB_DIR /usr/lib

ENV LIBZ_SYS_STATIC 1

RUN cargo build --release

RUN mkdir -p /release/bin
RUN mkdir -p /release/etc/vector
RUN cp -a target/release/vector /release/bin/
RUN cp -a config/. /release/etc/vector/

FROM alpine:3.10.1

RUN apk add g++ gcc

COPY --from=builder /release/bin/* /usr/local/bin/
COPY --from=builder /release/etc/vector /etc