FROM docker.io/rustembedded/cross:x86_64-unknown-linux-gnu

COPY bootstrap-rhel.sh .
RUN ./bootstrap-rhel.sh
