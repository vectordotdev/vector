# Wrapper around the official OpenTelemetry Collector image which lacks all basic utils which we want for debugging.
ARG CONFIG_COLLECTOR_VERSION=latest
FROM otel/opentelemetry-collector-contrib:${CONFIG_COLLECTOR_VERSION} AS upstream

FROM alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc AS base
COPY --from=upstream /otelcol-contrib /otelcol-contrib
COPY --from=upstream /etc/otelcol-contrib/config.yaml /etc/otelcol-contrib/config.yaml

# Run as root by default so we can write to the output volume.
USER root

ENTRYPOINT ["/otelcol-contrib"]
CMD ["--config", "/etc/otelcol-contrib/config.yaml"]
