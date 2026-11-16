The GELF decoder now detects gzip/zlib-compressed frames by their magic bytes and decompresses them before JSON parsing. Datagram transports already did this in the `chunked_gelf` framing; stream transports (e.g. null-delimited GELF over TCP from the `socket` source) previously failed with a JSON parse error and dropped the message. Decompression reuses `CappedDecoder`, so the shared decompression-bomb size cap applies.

authors: yackushevas
