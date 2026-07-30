The `metric_tag_values` option on the `remap` transform now accepts an `auto` value that
exposes single-value tags as strings and multi-value tags as arrays -- preserving the
underlying shape of each tag instead of forcing every tag into one form. The `lua` transform
continues to support only `single` and `full`.

authors: kaarolch
