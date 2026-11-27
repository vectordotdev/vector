The `throttle` transform can now apply different rate limits to each service or other
`key_field` group. Set `threshold_field` to the path of a field containing the event's
limit. Events with a missing or invalid value use the configured `threshold`.

authors: jonahgetz
