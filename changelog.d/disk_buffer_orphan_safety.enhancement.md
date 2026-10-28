Vector now reports disk buffer lifecycle events during startup and configuration reloads. Each event is logged and increments the `topology_disk_buffer_lifecycle_events_total` counter with an `event` tag:

- `removed`: A removed sink still had data in its disk buffer, or Vector could not read how much data it held. The log says whether the sink's current instance keeps draining the buffer in the background or whether the reload waits for it to finish.
- `replacement_unreferenced`: A changed sink no longer uses its previous disk buffer. If the reload succeeds, the new sink will not read any data left in the old buffer.
- `unreferenced`: A disk buffer in the data directory is not used by the current configuration. It may still be draining through a detached sink instance. If not, any data it holds is not being processed.

authors: Jansen-w
