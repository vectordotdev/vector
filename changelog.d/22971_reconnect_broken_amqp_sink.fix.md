The `amqp` sink now attempts to re-connect to the AMQP broker when the channel has been disconnected. It will also create up to 4 channels in a pool (configurable with the `max_channels` setting) to improve throughput.

authors: aramperes
