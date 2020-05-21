FROM debian:buster-slim

RUN apt-get update && apt-get install -y ca-certificates tzdata && rm -rf /var/lib/apt/lists/*

ENTRYPOINT ["/usr/bin/vector"]

ARG LOG=debug,vector=trace
ENV LOG="$LOG"

COPY vector /usr/bin/vector
