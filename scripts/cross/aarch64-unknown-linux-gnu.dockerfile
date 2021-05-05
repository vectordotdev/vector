FROM docker.io/rustembedded/cross:aarch64-unknown-linux-gnu

COPY bootstrap.sh .
RUN ./bootstrap.sh
