FROM ghcr.io/open-telemetry/opentelemetry-collector-contrib/telemetrygen:v0.137.0 AS telemetrygen

# Use alpine as base to get shell utilities
FROM alpine:3.20

# Install netcat for port checking
RUN apk add --no-cache netcat-openbsd

# Copy telemetrygen binary from official image
COPY --from=telemetrygen /telemetrygen /usr/local/bin/telemetrygen

# Set entrypoint to shell so we can run wrapper scripts
ENTRYPOINT ["/bin/sh"]
