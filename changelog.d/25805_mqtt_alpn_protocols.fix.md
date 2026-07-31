The `mqtt` sink and source now honor the configured `tls.alpn_protocols` option instead of always advertising the hardcoded `mqtt` ALPN protocol. This allows connecting to endpoints that require a specific ALPN protocol name, such as AWS IoT Core over port 443 which requires `x-amzn-mqtt-ca`. When `tls.alpn_protocols` is not set, the previous `mqtt` default is preserved.

authors: frank-hivewatch
