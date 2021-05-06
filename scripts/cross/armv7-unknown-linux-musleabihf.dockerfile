FROM docker.io/rustembedded/cross:armv7-unknown-linux-musleabihf-0.2.1

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
