# Wrapper around the official OpenTelemetry Collector image which lacks all basic utils which we want for debugging.
ARG COLLECTOR_IMAGE=otel/opentelemetry-collector-contrib:0.159.0@sha256:1f2c54a30e713fac6b3ae77a1ec84010c2007e29ced8ec666214fc2f6739c1cc
FROM ${COLLECTOR_IMAGE} AS upstream

FROM alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc AS base
COPY --from=upstream /otelcol-contrib /otelcol-contrib
COPY --from=upstream /etc/otelcol-contrib/config.yaml /etc/otelcol-contrib/config.yaml

# Run as root by default so we can write to the output volume.
USER root

ENTRYPOINT ["/otelcol-contrib"]
CMD ["--config", "/etc/otelcol-contrib/config.yaml"]
