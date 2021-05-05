FROM docker.io/rustembedded/cross:aarch64-unknown-linux-musl

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
