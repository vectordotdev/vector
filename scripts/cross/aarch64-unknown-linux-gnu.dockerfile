FROM docker.io/rustembedded/cross:aarch64-unknown-linux-gnu

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
