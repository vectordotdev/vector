Add a new `window` transform, a variant of ring buffer or backtrace logging implemented as a sliding window.
Allows for reduction of log volume by filtering out logs when the system is healthy, but preserving detailed
logs when they are most relevant.

authors: ilinas
