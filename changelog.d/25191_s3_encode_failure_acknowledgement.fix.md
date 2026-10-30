Sinks now reject batches that fail encoding instead of acknowledging them as delivered. This prevents upstream sources with end-to-end acknowledgements enabled from losing events silently.

authors: pedramsafaei
