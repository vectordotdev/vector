FROM ghcr.io/cross-rs/arm-unknown-linux-gnueabi:0.2.5

COPY scripts/cross/bootstrap-ubuntu.sh scripts/environment/install-protoc.sh /
RUN /bootstrap-ubuntu.sh && bash /install-protoc.sh
