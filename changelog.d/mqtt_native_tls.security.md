The `mqtt` source and sink now use the platform TLS library (OpenSSL on Linux, Security.framework on macOS, SChannel on Windows) instead of Rustls. This removes `rustls-webpki` 0.102, which has no patched release for RUSTSEC-2026-0049, RUSTSEC-2026-0098, RUSTSEC-2026-0099, RUSTSEC-2026-0104, and RUSTSEC-2026-0382. When `tls.ca_file` is not set, the system root certificates are now trusted instead of every connection failing. On macOS, broker certificates must follow Apple's TLS certificate policy, which rejects server certificates valid for more than 825 days.

authors: thomasqueirozb
