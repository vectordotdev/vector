FROM ghcr.io/cross-rs/armv7-unknown-linux-gnueabihf:0.2.4

COPY scripts/cross/bootstrap-ubuntu.sh scripts/environment/install-protoc.sh /
RUN /bootstrap-ubuntu.sh && bash /install-protoc.sh
