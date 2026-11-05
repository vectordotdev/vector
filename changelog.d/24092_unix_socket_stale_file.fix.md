Unix socket sources (`socket` in `unix_datagram` or `unix_stream` mode, and other sources that listen on a Unix stream socket) now remove a stale socket file left behind by a previous Vector process that did not shut down cleanly, for example after an OOM kill.
Before, the source panicked with `Address in use` until the file was removed manually.
A socket file that still has a listener is left in place.

authors: hamishforbes
