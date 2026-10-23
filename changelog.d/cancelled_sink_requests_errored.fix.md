Sinks now report events as errored when a request is cancelled before it completes, for example when `--reload-sink-drain-limit-secs` expires during a configuration reload. Before, these events were reported as delivered, so sources with end-to-end acknowledgements acknowledged data that was not sent. These sources can now retry the events instead.

authors: thomasqueirozb
