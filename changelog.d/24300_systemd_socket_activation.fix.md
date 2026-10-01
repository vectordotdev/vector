Fix collection from systemd sockets.

Systemd sockets are passed in blocking mode, but tokio expects them to be in non-blocking mode.
Therefore, always set sockets from systemd to non-blocking.

authors: j-c-fuchs aagor
