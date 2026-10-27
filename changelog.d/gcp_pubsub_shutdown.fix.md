The `gcp_pubsub` source no longer delays Vector shutdown when the shutdown starts while the source
connects, waits to retry, or holds an open stream with acknowledgements enabled.

authors: thomasqueirozb
