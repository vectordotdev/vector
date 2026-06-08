The `reduce` transform now accepts a `data_type` option (`log` or
`trace`, default `log`) that selects whether the instance collapses log
events or trace events. The existing merge strategies and conditions
apply unchanged to trace events.

authors: p120ph37
