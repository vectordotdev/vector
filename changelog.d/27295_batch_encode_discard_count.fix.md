Sinks that encode a batch of events now report every event discarded when one event fails to encode. Previously only the failing event was counted, so the remaining events in the batch were lost from both `component_sent_events_total` and `component_discarded_events_total`.

authors: Goldloli
