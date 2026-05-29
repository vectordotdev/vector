UDP and Unix datagram sinks now reconnect and retry the failed event after a socket send failure instead of silently dropping all remaining events on the broken socket.

authors: thomasqueirozb
