Fix reporting of configured limits for in-memory buffers using `max_size` (byte-based): the
`buffer_max_size_events` gauge was incorrectly set to the byte limit, and the
`buffer_max_size_bytes` gauge was not set at all. Byte limits are now reported through
`buffer_max_size_bytes`, leaving `buffer_max_size_events` unset, matching the behavior of disk
buffers and event-based (`max_events`) memory buffers.

authors: yackushevas
