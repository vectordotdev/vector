Fixed a bug in the `mqtt` source where user-provided TLS client certificates (`crt_file` / `key_file`) were being silently ignored, breaking mTLS connections to strict brokers like AWS IoT Core.

authors: mr-
