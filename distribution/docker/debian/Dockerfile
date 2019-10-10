FROM debian:buster-slim
RUN apt-get update && apt-get install -y ca-certificates

COPY vector-amd64.deb .
RUN dpkg -i vector-amd64.deb
RUN rm /vector-amd64.deb

ENTRYPOINT ["/usr/bin/vector"]
