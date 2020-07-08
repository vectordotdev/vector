FROM debian:buster-slim AS builder

WORKDIR /vector

COPY vector-*-unknown-linux-gnu*.tar.gz ./
RUN tar -xvf vector-$(uname -m)-unknown-linux-gnu*.tar.gz --strip-components=2

FROM debian:buster-slim

RUN apt-get update \
  && apt-get install -y \
    ca-certificates \
    tzdata \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /vector/bin/vector /usr/local/bin/vector
COPY --from=builder /vector/config/vector.toml /etc/vector/vector.toml

ENTRYPOINT ["/usr/local/bin/vector"]
