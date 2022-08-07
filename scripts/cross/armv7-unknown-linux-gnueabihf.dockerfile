FROM ghcr.io/cross-rs/armv7-unknown-linux-gnueabihf:0.2.4

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
