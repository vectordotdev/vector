FROM docker.io/rustembedded/cross:x86_64-unknown-freebsd

# freebsd image is actually based on Ubuntu and copies over FreeBSD libraries

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
