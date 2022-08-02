FROM ghcr.io/cross-rs/powerpc64le-unknown-linux-gnu:0.2.4

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
