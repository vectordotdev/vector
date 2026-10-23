Fixed the `reduce` transform dropping fields whose names contain `"` or `\`, and renaming fields
whose names contain `\\` or `\"`. These fields are now merged like any other field, and the
transform no longer logs `Event field could not be reduced.` or increments
`component_errors_total` for them. Field paths are also built directly from the event instead of
being formatted as strings and parsed back.

authors: ckelner
