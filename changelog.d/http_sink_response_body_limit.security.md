HTTP-based sinks now bound the total memory used for buffering downstream response bodies to
1 GiB per sink. Previously, a destination returning an arbitrarily large or slow, never-ending
response could cause Vector to buffer response bodies in memory without limit, eventually
exhausting available memory and terminating the process. When the bound is reached, the sink
pauses reading responses and stops accepting new batches, applying backpressure until buffered
responses complete. Response bodies are never truncated or rejected based on size; individual
responses of any size still complete.

authors: thomasqueirozb
