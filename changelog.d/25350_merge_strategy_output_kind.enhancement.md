The `reduce` transform's `MergeStrategy` type now exposes a public `output_kind` method
that returns the output `Kind` produced by applying a given strategy to a field. This
allows downstream consumers wrapping the reduce transform to compute output schema
definitions without duplicating the per-strategy type inference logic.

authors: sghall
