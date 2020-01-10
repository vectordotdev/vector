FROM debian:buster-slim AS builder

COPY vector-*.deb ./
RUN dpkg -i vector-$(dpkg --print-architecture).deb

FROM debian:buster-slim

RUN apt-get update && apt-get install -y ca-certificates tzdata && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/bin/vector /usr/bin/vector
COPY --from=builder /usr/share/doc/vector /usr/share/doc/vector
COPY --from=builder /etc/vector /etc/vector

ENTRYPOINT ["/usr/bin/vector"]
