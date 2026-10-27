# Distroless does not use static tags.
# hadolint ignore=DL3007
FROM gcr.io/distroless/cc-debian13:latest@sha256:d74d144baa124dd9defd01d71872bb56c49d4441dba6153ccf790364f97e64b1 AS distroless

FROM docker.io/debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad AS builder

WORKDIR /vector

COPY vector_*.deb ./
RUN dpkg -i vector_*_"$(dpkg --print-architecture)".deb

RUN mkdir -p /var/lib/vector

# Stage libz at its arch-specific path so it can be copied into the distroless
# runtime. distroless/cc does not include zlib; vector (via rdkafka) links
# against it dynamically and we deliberately keep it as a runtime dependency.
RUN src=$(dpkg -L zlib1g | grep -E '/libz\.so\.1$') && \
    install -D "$(realpath "$src")" "/staging${src}"

# Remove the revoked emSign roots C1, C3, and G3 (CVE-2026-111943).
COPY --from=distroless /etc/ssl/certs/ca-certificates.crt /tmp/base.crt
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends openssl && rm -rf /var/lib/apt/lists/* \
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

COPY --from=builder /usr/bin/vector /usr/bin/vector
COPY --from=builder /usr/share/doc/vector /usr/share/doc/vector
COPY --from=builder /usr/share/vector /usr/share/vector
COPY --from=builder /etc/vector /etc/vector
COPY --from=builder /var/lib/vector /var/lib/vector
COPY --from=builder /staging/ /
COPY --from=builder /ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
