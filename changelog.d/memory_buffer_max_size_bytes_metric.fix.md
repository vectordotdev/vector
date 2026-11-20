Memory buffers configured with `max_size` now report their limit as `buffer_max_size_bytes`. Previously
the byte limit was reported as `buffer_max_size_events`, and `buffer_max_size_bytes` was never emitted
for them.

authors: armleth
