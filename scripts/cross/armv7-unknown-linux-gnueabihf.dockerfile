FROM rustembedded/cross:armv7-unknown-linux-gnueabihf

COPY bootstrap.sh .
RUN ./bootstrap.sh
