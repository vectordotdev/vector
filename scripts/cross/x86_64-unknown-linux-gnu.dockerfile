FROM ghcr.io/cross-rs/x86_64-unknown-linux-gnu:0.2.4

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
