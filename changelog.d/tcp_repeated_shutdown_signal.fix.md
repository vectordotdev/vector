Fixed a bug in TCP sources where, once `max_connection_duration_secs` elapsed for a connection, Vector would repeatedly re-issue `shutdown(SHUT_WR)` on every subsequent poll instead of just once.

authors: tronboto
