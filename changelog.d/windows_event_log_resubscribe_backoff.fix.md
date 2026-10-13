The `windows_event_log` source no longer rebuilds a subscription every `event_timeout_ms` when a disabled or empty
channel keeps reporting an unusable handle. Rebuilds of a channel that delivers nothing now back off (5 s, doubling,
up to 10 minutes) and only the first and every power-of-two attempt is logged at `warn`; a signalled channel is
rebuilt immediately, so real events are not delayed.

authors: pos-ei-don
