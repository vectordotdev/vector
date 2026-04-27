The NATS JetStream source now automatically recovers when the pull stream terminates due to a connection close event (e.g., during NATS rolling upgrades or lame duck mode). Previously, the source would silently stop consuming messages. It now reconnects and rebuilds the pull consumer stream with exponential backoff.

authors: benjamin-awd
