The `remap` transform and VRL and Datadog Search conditions (used by `filter`, `route`, and
`exclusive_route`) now share their compiled program across concurrent tasks instead of deep-copying
it for every batch of events. This reduces CPU and allocations for large VRL programs and search
queries.

authors: gwenaskell
