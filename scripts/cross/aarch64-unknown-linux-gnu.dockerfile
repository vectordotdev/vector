FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
