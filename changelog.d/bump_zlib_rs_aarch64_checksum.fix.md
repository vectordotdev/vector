Updated the zlib backend (`zlib-rs`) from 0.6.0 to 0.6.6, pulling in an upstream fix for an aarch64 NEON Adler-32 bug that could produce incorrect checksums. On arm64, valid zlib-format (RFC 1950) payloads, such as `deflate` content-encoding handled by HTTP-based sources, could previously be rejected as corrupt with an `incorrect data check` error.

authors: Stunned1
