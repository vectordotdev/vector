Reduce the per-event memory allocations of the `kubernetes_logs` source.

The source rebuilt the per-pod labels of `component_received_events_total` and
`component_received_event_bytes_total` for every log line, and it converted the
pod name and namespace into new `String`s per line for an internal trace event.
The counter handles are now created once per pod and cached, and those two strings
are no longer allocated per event. The counters, their labels, and the event byte
size they report are unchanged.

authors: thomasqueirozb
