FROM docker.io/rustembedded/cross:x86_64-unknown-linux-gnu

COPY bootstrap.sh .
RUN ./bootstrap.sh
