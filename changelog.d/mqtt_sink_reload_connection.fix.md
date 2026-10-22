The `mqtt` sink now closes its broker connection after the sink stops, for example when a configuration reload removes or replaces it. Before, the connection stayed open after the sink stopped. A replacement sink that uses the same client ID could then be repeatedly disconnected by the broker.

After the sink stops, its connection stays open for up to five seconds to send queued messages and complete QoS acknowledgements. The connection closes sooner if a connection error occurs.

authors: thomasqueirozb
