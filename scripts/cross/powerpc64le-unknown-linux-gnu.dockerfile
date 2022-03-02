FROM docker.io/rustembedded/cross:powerpc64le-unknown-linux-gnu

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
