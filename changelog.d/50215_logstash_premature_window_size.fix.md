The `logstash` source now rejects a `WindowSize` frame that arrives before the
current window has received all of its advertised events, closing the connection
with a fatal decode error instead of making any attempt to continue. While this
is allowed by the protocol spec, no known client makes use of this and the
reference server in `go-lumber` treats it as a protocol violation.

authors: bruceg
