The `windows_event_log` source no longer rebuilds the subscription of a disabled or empty channel every
`event_timeout_ms`. On such a channel `EvtNext` returns `ERROR_INVALID_OPERATION` because there is nothing to read;
the subscription is now kept instead of rebuilt (checked via the channel's enabled flag and record count). Rebuilds of a
channel that delivers nothing also back off (5 s, doubling, up to 10 minutes), and only the first and every power-of-two
attempt is logged at `warn`. A signalled channel is handled immediately, so real events are never delayed.

authors: pos-ei-don
