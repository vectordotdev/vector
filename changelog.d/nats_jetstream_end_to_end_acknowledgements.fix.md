Fix the `nats` source's JetStream mode to acknowledge messages only after all connected sinks
confirm delivery when end-to-end acknowledgements are enabled. Failed deliveries are negatively
acknowledged for redelivery. Core NATS behavior is unchanged.
JetStream consumers must use the `explicit` acknowledgement policy when end-to-end
acknowledgements are enabled.

authors: horjulf
