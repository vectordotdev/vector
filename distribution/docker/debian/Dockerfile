FROM debian:buster-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY vector-amd64.deb .
RUN dpkg -i vector-amd64.deb
RUN rm /vector-amd64.deb

ENTRYPOINT ["/usr/bin/vector"]
