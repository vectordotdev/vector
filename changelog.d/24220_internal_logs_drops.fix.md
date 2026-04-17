Fixed the `internal_logs` source silently dropping events under high load. Events that are still dropped when the underlying broadcast receiver lags now increment the standard `component_discarded_events_total{intentional="false"}` metric so the loss is observable.

authors: thomasqueirozb
