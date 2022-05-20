FROM ghcr.io/cross-rs/x86_64-unknown-linux-musl:main

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
