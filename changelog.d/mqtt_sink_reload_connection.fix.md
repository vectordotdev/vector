The `mqtt` sink now closes its broker connection after the sink stops, for example when a configuration reload removes or replaces it. Before, the connection stayed open after the sink stopped. A replacement sink that uses the same client ID could then be repeatedly disconnected by the broker.

authors: thomasqueirozb
