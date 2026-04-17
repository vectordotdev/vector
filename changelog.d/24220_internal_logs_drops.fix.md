Fixed the `internal_logs` source silently dropping events under high load. The internal broadcast channel buffer used to fan out tracing events to subscribers was raised from 99 to 10000, and any events that are still dropped due to lag now increment the standard `component_discarded_events_total{intentional="false"}` metric so the loss is observable.

authors: thomasqueirozb
