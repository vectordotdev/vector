`vector tap` no longer receives duplicate events after configuration reloads that leave a tapped
component unchanged, and stops tapping outputs that no longer match the requested patterns. Events
dropped because the tap client cannot keep up are now reported through the
`component_discarded_events_total` metric.

authors: 20agbekodo
