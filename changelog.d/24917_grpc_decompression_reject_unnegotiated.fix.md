The shared gRPC decompression layer now rejects request frames that set the
compressed flag without a negotiated `grpc-encoding` (e.g. `identity` or a
missing header). Previously such malformed frames were silently decoded as
gzip, which could mask client/server compression-negotiation bugs.

authors: jpds
