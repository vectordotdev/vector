The `character_delimited` and `newline_delimited` decoding framers now emit
`component_discarded_events_total` with `intentional="true"` when a frame exceeding `max_length` is
dropped (`oversized_action: drop`). Dropping behavior is otherwise unchanged.

authors: taylorchandleryoung
