FROM docker.io/rustembedded/cross:armv7-unknown-linux-musleabihf

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
