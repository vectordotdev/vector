Fixed bugs in the `gcp_pubsub` source where the source could fail to shut down cleanly when
acknowledgements were enabled. The source now keeps the finalizer alive long enough to drain
pending acknowledgements, and stops accepting new Pub/Sub batches after shutdown begins so it
can finish shutting down under backlog or continuous publishing.

authors: thomasqueirozb
