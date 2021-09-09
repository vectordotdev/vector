FROM debian:bullseye-slim AS builder

COPY vector-*.deb ./
RUN dpkg -i vector-*-$(dpkg --print-architecture).deb

FROM gcr.io/distroless/cc-debian10

COPY --from=builder /usr/bin/vector /usr/bin/vector
COPY --from=builder /usr/share/doc/vector /usr/share/doc/vector
COPY --from=builder /etc/vector /etc/vector
VOLUME /var/lib/vector/

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
