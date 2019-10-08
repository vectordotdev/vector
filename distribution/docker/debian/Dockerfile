FROM debian:buster-slim

COPY vector-amd64.deb .
RUN dpkg -i vector-amd64.deb
RUN rm /vector-amd64.deb

ENTRYPOINT ["/usr/bin/vector"]
