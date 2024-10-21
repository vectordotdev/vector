FROM docker.io/alpine:3.20 AS builder

WORKDIR /vector

ARG TARGETPLATFORM

COPY vector-*-unknown-linux-musl*.tar.gz ./

# special case for arm v6 builds, /etc/apk/arch reports armhf which conflicts with the armv7 package
RUN ARCH=$(if [ "$TARGETPLATFORM" = "linux/arm/v6" ]; then echo "arm"; else cat /etc/apk/arch; fi) \
    && tar -xvf vector-0*-"$ARCH"-unknown-linux-musl*.tar.gz --strip-components=2

RUN mkdir -p /var/lib/vector

FROM docker.io/alpine:3.20
# we want the latest versions of these
# hadolint ignore=DL3018
RUN apk --no-cache add ca-certificates tzdata

COPY --from=builder /vector/bin/* /usr/local/bin/
COPY --from=builder /vector/config/vector.yaml /etc/vector/vector.yaml
COPY --from=builder /var/lib/vector /var/lib/vector

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/local/bin/vector"]
