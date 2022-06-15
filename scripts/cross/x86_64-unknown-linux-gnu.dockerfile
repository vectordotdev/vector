FROM docker.io/rustembedded/cross:x86_64-unknown-linux-gnu

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
