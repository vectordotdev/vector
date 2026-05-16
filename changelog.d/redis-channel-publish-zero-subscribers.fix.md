Fixed an issue where the `redis` sink using `data_type: channel` would enter an infinite retry
loop when a `PUBLISH` command returned `0` (no subscribers currently connected). Redis pub/sub
`PUBLISH` returns the number of subscribers that received the message; zero subscribers is a
valid transient state (e.g. the consumer momentarily dropped its subscription) and should not
be treated as a failure.
