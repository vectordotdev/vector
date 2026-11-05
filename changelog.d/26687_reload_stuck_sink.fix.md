Config reloads no longer hang when a changed sink stops polling its input, such as a `socket` sink retrying a connection that keeps failing. Vector now waits up to the graceful shutdown limit (`--graceful-shutdown-limit-secs`) for the old sink to finish, then drops it, along with any events it had already taken from its buffer, and starts the new sink on the same buffer. Previously the reload waited forever, and Vector ignored SIGTERM while it did.

authors: hathawsh
