FROM docker.io/debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad AS builder

WORKDIR /vector

COPY vector_*.deb ./
RUN dpkg -i vector_*_"$(dpkg --print-architecture)".deb

RUN mkdir -p /var/lib/vector

FROM docker.io/debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad

# https://github.com/opencontainers/image-spec/blob/main/annotations.md
LABEL org.opencontainers.image.url="https://vector.dev"
LABEL org.opencontainers.image.source="https://github.com/vectordotdev/vector"
LABEL org.opencontainers.image.documentation="https://vector.dev/docs"

# we want the latest versions of these
# Remove the revoked emSign roots C1, C3, and G3 (CVE-2026-111943).
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata systemd libsasl2-2 && rm -rf /var/lib/apt/lists/* \
    && sed -i '\|^mozilla/emSign_Root_CA_-_C1\.crt$|d; \|^mozilla/emSign_ECC_Root_CA_-_C3\.crt$|d; \|^mozilla/emSign_ECC_Root_CA_-_G3\.crt$|d' /etc/ca-certificates.conf \
    && update-ca-certificates --fresh \
    && openssl storeutl -noout -text -certs /etc/ssl/certs/ca-certificates.crt > /tmp/ca.txt \
    && ! grep -E 'Subject:.*CN=emSign (Root CA - C1|ECC Root CA - C3|ECC Root CA - G3)$' /tmp/ca.txt \
    && rm /tmp/ca.txt

COPY --from=builder /usr/bin/vector /usr/bin/vector
COPY --from=builder /usr/share/vector /usr/share/vector
COPY --from=builder /usr/share/doc/vector /usr/share/doc/vector
COPY --from=builder /etc/vector /etc/vector
COPY --from=builder /var/lib/vector /var/lib/vector

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
