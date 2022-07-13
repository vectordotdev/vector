FROM ghcr.io/cross-rs/armv7-unknown-linux-gnueabihf:main

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
