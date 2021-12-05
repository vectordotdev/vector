FROM rustembedded/cross:armv7-unknown-linux-gnueabihf

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
