Fixed a panic in the tracing rate limiter when config reload failed. While the panic didn't kill Vector (it was caught by tokio's task
runtime), it could cause unexpected behavior. The rate limiter now gracefully handles events without standard message fields.

authors: pront
