The `metric_tag_values` option (used by the `remap` and `lua` transforms) now accepts an
`auto` value that exposes single-value tags as strings and multi-value tags as arrays --
preserving the underlying shape of each tag instead of forcing every tag into one form.

authors: kaarolch
