During a configuration reload, Vector now limits how long removed and changed sinks can take to finish their work. The limit comes from `--graceful-shutdown-limit-secs` (default 60 seconds), and all of these sinks share one deadline. Before, there was no limit, and a reload could wait indefinitely for a sink that did not stop. To wait without a limit, use `--no-graceful-shutdown-limit`.

When a sink reaches the deadline, Vector cancels its in-flight work. If the replacement sink reuses the buffer, it receives the events that the old sink did not read.

authors: thomasqueirozb
