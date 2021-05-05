FROM docker.io/rustembedded/cross:aarch64-unknown-linux-musl

COPY bootstrap.sh .
RUN ./bootstrap.sh
