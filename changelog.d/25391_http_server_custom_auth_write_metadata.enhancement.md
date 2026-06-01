The `custom` auth strategy for the `http_server` source now supports event enrichment via metadata
writes. VRL programs can write `%field = value` during authentication; those values are injected
into every successfully authenticated event. The event body (`.field`) remains read-only. Existing
`custom` programs that do not write metadata are unaffected.

authors: 20agbekodo
