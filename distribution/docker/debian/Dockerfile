FROM debian:bullseye-slim AS builder

COPY vector_*.deb ./
RUN dpkg -i vector_*_$(dpkg --print-architecture).deb

RUN mkdir -p /var/lib/vector

FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y ca-certificates tzdata systemd && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/bin/vector /usr/bin/vector
COPY --from=builder /usr/share/doc/vector /usr/share/doc/vector
COPY --from=builder /etc/vector /etc/vector
COPY --from=builder /var/lib/vector /var/lib/vector

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
