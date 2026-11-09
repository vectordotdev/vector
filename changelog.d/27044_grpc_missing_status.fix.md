An HTTP 200 stream that ends without a `grpc-status` trailer now reports `Unknown` instead of
looking like normal stream completion. This affects handling of truncated responses, including
proxy or load-balancer resets.

authors: pront
