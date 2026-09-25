The `reduce` transform now accepts and emits `trace` events in addition to
`log` events. The event type is detected automatically, and events of
different types are never merged together, so a `reduce` instance fed both
logs and traces reduces each type independently. The existing merge
strategies and conditions apply unchanged to trace events.

authors: p120ph37
