The `sample` transform now exposes an optional `with_discarded_event_tags`
builder method on the `Sample` runtime type. When set, the callback returns
`(key, value)` pairs that are merged into the `component_discarded_events_total`
counter at the drop site. Default `None`; existing users see no behavior change.

Library consumers (e.g., downstream wrappers that pre-group events) can use
this to attach per-event tags to the discard metric without forking the
transform.
