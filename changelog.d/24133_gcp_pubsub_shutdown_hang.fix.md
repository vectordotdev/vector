Fixed a bug in the `gcp_pubsub` source where the source could hang indefinitely on shutdown
when acknowledgements were enabled and a shutdown signal arrived while event batches were
still pending acknowledgement. The internal finalizer stream terminated prematurely on
shutdown, leaving pending acks unprocessable and preventing the shutdown condition from
being satisfied.

authors: thomasqueirozb
