FROM docker.io/rustembedded/cross:armv7-unknown-linux-musleabihf

COPY bootstrap.sh .
RUN ./bootstrap.sh
