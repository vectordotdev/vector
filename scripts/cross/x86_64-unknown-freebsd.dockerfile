FROM ghcr.io/cross-rs/x86_64-unknown-freebsd:main

# freebsd image is actually based on Ubuntu and copies over FreeBSD libraries

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh
