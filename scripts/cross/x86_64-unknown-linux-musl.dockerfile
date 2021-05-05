FROM docker.io/rustembedded/cross:x86_64-unknown-linux-musl

COPY bootstrap.sh .
RUN ./bootstrap.sh
