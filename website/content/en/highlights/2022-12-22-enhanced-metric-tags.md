---
date: "2022-12-22"
title: "Enhanced Metric Tags"
description: "The metric data model now supports an enhanced representation of tag values"
authors: ["bruceg"]
pr_numbers: [14742]
release: "0.27.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

The existing set of tags that have been available on metrics was a simple mapping of string values
to single string keys. Some sources, however, may produce metrics that contain tags that don't fit
into this model, such as tags that are bare strings (rather than a key/value pair) or have multiple
values for a given key.

With this release, tag values for a given key may now contain a set of values, each of which may be
a string value or `null`, which represents a "bare" tag (i.e. a tag with no value, as distinct from
an empty string value). This new tag set only stores unique values, as duplicated tags are not
useful for any component, and so each tag name/value pair will appear only once across all tags.

For compatibility with previous releases, this value set also tracks the last seen or assigned value
for each tag. Since most components, in particular sinks, can only make use of a single value for
each tag, this last value is used by those components.

The scripting components of `lua` and `remap` have been extended with a configuration option of
`metric_tag_values`. This controls how these tag values are exposed to scripts. In the default
setting of `single`, tag values will be exposed as the single value described above. This behavior
matches the existing behavior of scripts so that no script changes will be needed. When set to
`full`, however, all tag values are exposed as an array for each element. In either case, scripts may
assign either a single value or an array of values to a tag.

For example:

```coffee
.tags.host = "localhost" # Assign a single string value
.tags.bare = null # Create a single-valued bare tag
.tags.complex = ["remotehost", null, "otherhost"] # Creates three tag values
```

This `metric_tag_values` setting also shows up in the `codec` configuration of sinks and controls
how metric tags are exposed in codecs that can encode metrics. As above, when set to `full`, these
codecs will expose all values of tags either as arrays (for the `json` codec) or as repeated
instances of each tag (for the `text` codec). This setting does not apply to the `native` and
`native_json` codecs which _always_ expose all tag values in order to seamlessly transport the
values to other Vector instances.
