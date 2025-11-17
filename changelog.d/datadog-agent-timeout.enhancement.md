Added support for configurable request timeouts to the `datadog_agent` source.

This change also introduces two new internal metrics:
	- `component_timed_out_events_total` - Counter tracking the number of events that timed out
	- `component_timed_out_requests_total` - Counter tracking the number of requests that timed out

authors: bruceg
