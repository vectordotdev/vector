FROM ghcr.io/open-telemetry/opentelemetry-collector-contrib/telemetrygen:v0.137.0@sha256:068ec2c940babb71d1c621057e104a4a736ea96ce66967353384da9225114e76 AS telemetrygen

# Use alpine as base to get shell utilities
FROM alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc

# Install netcat for port checking
RUN apk add --no-cache netcat-openbsd

# Copy telemetrygen binary from official image
COPY --from=telemetrygen /telemetrygen /usr/local/bin/telemetrygen

# Set entrypoint to shell so we can run wrapper scripts
ENTRYPOINT ["/bin/sh"]
