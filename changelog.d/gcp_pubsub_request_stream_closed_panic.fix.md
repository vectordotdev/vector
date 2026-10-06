The `gcp_pubsub` source no longer panics with `internal error: entered unreachable code: request stream never closes` when Pub/Sub closes a streaming pull (for example with an HTTP/2 `GOAWAY`) while acknowledgements or a keepalive are being sent. The stream is now restarted instead; acknowledgements that could not be sent are redelivered by Pub/Sub after the ack deadline. Previously the panicking stream task stopped pulling until Vector was restarted.

authors: Harish-Narayan
