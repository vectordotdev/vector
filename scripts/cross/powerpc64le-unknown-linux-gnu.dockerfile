FROM ghcr.io/cross-rs/powerpc64le-unknown-linux-gnu:main

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
