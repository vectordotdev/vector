# Distroless does not use static tags.
# hadolint ignore=DL3007
FROM gcr.io/distroless/static:latest@sha256:15f8637509792e05b97d83451b0b6ccfa4abd548f997e406ac1e72c87fe1fa78 AS distroless

FROM docker.io/alpine:3.24@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a AS builder

WORKDIR /vector

COPY vector-*-unknown-linux-musl*.tar.gz ./
RUN tar -xvf vector-0*-"$(cat /etc/apk/arch)"-unknown-linux-musl*.tar.gz --strip-components=2

RUN mkdir -p /var/lib/vector

# Remove the revoked emSign roots C1, C3, and G3 (CVE-2026-111943).
COPY --from=distroless /etc/ssl/certs/ca-certificates.crt /tmp/base.crt
# hadolint ignore=DL3018
RUN apk --no-cache add openssl \
    && mkdir /tmp/ca \
    && awk '/BEGIN CERTIFICATE/ { if (f) close(f); f = sprintf("/tmp/ca/%04d.pem", ++n) } f { print > f }' /tmp/base.crt \
    && for f in /tmp/ca/*.pem; do \
         case "$(openssl x509 -noout -subject -in "$f")" in \
           *"CN=emSign Root CA - C1"|*"CN=emSign ECC Root CA - C3"|*"CN=emSign ECC Root CA - G3") ;; \
           *) cat "$f" ;; \
         esac; \
       done > /ca-certificates.crt \
    && openssl storeutl -noout -text -certs /ca-certificates.crt > /tmp/ca.txt \
    && ! grep -E 'Subject:.*CN=emSign (Root CA - C1|ECC Root CA - C3|ECC Root CA - G3)$' /tmp/ca.txt \
    && rm -r /tmp/ca /tmp/ca.txt /tmp/base.crt

FROM distroless

# https://github.com/opencontainers/image-spec/blob/main/annotations.md
LABEL org.opencontainers.image.url="https://vector.dev"
LABEL org.opencontainers.image.source="https://github.com/vectordotdev/vector"
LABEL org.opencontainers.image.documentation="https://vector.dev/docs"

COPY --from=builder /vector/bin/* /usr/local/bin/
COPY --from=builder /vector/config/vector.yaml /usr/share/vector/examples/vector.yaml
COPY --from=builder /var/lib/vector /var/lib/vector
COPY --from=builder /ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/local/bin/vector"]
