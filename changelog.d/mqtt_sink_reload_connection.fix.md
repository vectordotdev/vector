The `mqtt` sink now closes its broker connection when the sink stops, for example when a configuration reload removes or replaces it. Before, the connection stayed open, and the broker could repeatedly disconnect a replacement sink that used the same client ID.

Before it stops, the sink sends its queued messages and completes pending QoS exchanges. If the connection fails during this step, the sink stops without a reconnect and the reload continues.

authors: thomasqueirozb
