FROM debian:buster-slim
RUN apt-get update && apt-get install -y ca-certificates tzdata && rm -rf /var/lib/apt/lists/*

COPY vector-*.deb ./
RUN dpkg -i vector-$(dpkg --print-architecture).deb
RUN rm /vector-$(dpkg --print-architecture).deb

ENTRYPOINT ["/usr/bin/vector"]
