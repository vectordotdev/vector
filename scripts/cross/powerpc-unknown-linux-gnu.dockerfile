FROM rustembedded/cross:powerpc-unknown-linux-gnu

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
