FROM ghcr.io/cross-rs/powerpc-unknown-linux-gnu:main

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
