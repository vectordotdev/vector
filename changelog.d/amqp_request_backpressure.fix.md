The `amqp` sink now bounds pending publish requests using `max_channels`, preventing
unbounded request accumulation when channels or publisher confirmations are unavailable.
Each sink has its own limit.

authors: mojish
