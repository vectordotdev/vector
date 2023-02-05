# RFC 14742 - Support for Duplicate and Bare Tags on Metrics

Vector's current metric model supports only a single value for each tag. Several sources, however,
may send metrics that contain multiple values for a given tag name, or bare tags with no
value. Vector's tag support should be enhanced to handle this data.

## Context

- [Support duplicate tag and bare tags on metrics](https://github.com/vectordotdev/vector/issues/14742)
- [DataDog Source -> Filter Transform -> DataDog Sink dropping kube_service tag](https://github.com/vectordotdev/vector/issues/14707)
- [Invalid custom_tags metrics from sinks datadog_metric](https://github.com/vectordotdev/vector/issues/14239)

## Scope

### In scope

- Changes to the existing metric tag internal and external representations to support multiple
  values and bare tags.

### Out of scope

- Changes to any other aspect of metric or event representations.

## Pain

When ingesting data from certain metric sources, the tags provided by that source may contain data
that Vector's data model cannot currently represent. This is causing problems for prospective users
that want to add Vector to their observability pipelines but cannot do so because it would cause
data loss. Those sources include:

- `datadog_agent` natively stores tags as bare strings, with optional values separated from the name
  by a colon.
- The Prometheus text encoding used by the `prometheus_scrape` source and `prometheus_exporter` sink
  can support both repeated tag names and bare tags (without a value).
- The Prometheus `remote_write` encoding similarly supports repeated tag names.
- OpenTelemetry attributes support keys having arrays of values, which corresponds to repeated tags.

## Proposal

### User Experience

#### External Representation

The existing Protobuf representation of metric tags has a map of string-to-string pairs of
tags. A new tags map will be added where the values are lists of optional strings, matching the
semantics given above.

```protobuf
message Metric {
  …
  map<string, string> tags_v1 = 3;
  message Values {
    message Value {
      optional string value = 1;
    }
    repeated Value values = 1;
  }
  map<string, Values> tags_v2 = 20;
  …
}
```

Similarly, the native JSON encoding for tags is a simple JSON object containing string-to-string
pairs. This will be enhanced to also allow the `null` value for bare tags, and arrays of either
strings or `null` for tags with more than one value.

```json
{
  "tags": {
    "single_value":"value",
    "bare_tag":null,
    "multi_valued_tag":["value1","value2"],
    "complex_tag":["value3",null]
  }
}
```

#### Scripting

Both the Lua and VRL scripting languages will be given a new configuration option named
`metric_tag_values` that controls how tag values are exposed to scripts. This may take two values,
`"single"` or `"full"`. When set to the former, tag values will be exposed as single strings, the
same as they are now. Tags with multiple values will show the last assigned value, and null values
will be ignored. When set to the latter, all tags will be exposed as arrays of either string or null
values.  This control will initially default to the former setting, providing for backwards
compatibility, but that default will later be deprecated and change to the latter.

Tag assignments will also follow the tag value mode. That is, only single value assignments will be
allowed in "single" mode, and only array value assignments will be allowed in "full" mode. In Lua,
emitted events with incompatible tag values will be dropped with an error as they are now. In VRL,
these types will be enforced with integrated type definitions, causing programs that make the wrong
assignment a config-level error instead of a run-time error. In any case, assignment to a tag will
overwrite all other values for the tag, and deleting a tag name or assigning an empty array (in
"full" mode) will remove all tag values with that name.

Since tags cannot have multiple identical values, and both scripting languages lack first-class
support for sets, assignments and modifications that result in duplicate values will cause the
duplicates to be dropped and a warning issued.

Examples:

```coffee
# With metric_tag_values = "single"
.tags.single_value = "value" # Replace the tag with a single value
.tags.bare_tag = null        # Replace the tag with a bare tag

# With metric_tag_values = "full":
.tags.multi_valued_tag = ["value1", "value2"]
.tags.complex_tag = ["value3", null]
.tags.modified = push(.tags.modified, "value4")
.tags.modified = filter(.tags.modified) -> |_, v| { v != "remove" }
```

#### `log_to_metric` Transform

The `log_to_metric` transform is configured with a set of tags for the output metrics. Existing
configurations with single value assignments will continue to work as they are currently
written. Support will be added for assignments of arrays to produce multi-valued tags and null
values to produce bare tags. As TOML does not support null values, the latter will be only be
supported in YAML and JSON configuration.

#### `tag_cardinality_limit` Transform

The `tag_cardinality_limit` transform tracks all values of tags, dropping either individual tags or
whole events when the cardinality exceeds a configured limit. There are three ways this will be
supported in the presence of multi-valued tags, which will be selectable with configuration options:

1. The individual values are tracked as before. Events are dropped when any one tag's cardinality
   exceeds the limit, but only the tags that would exceed the limit are dropped.
1. The individual values are tracked as before. Events are dropped when any one tag's cardinality
   exceeds the limit, and all values of tags that would exceed the limit are dropped.
1. Values of multi-valued tags are combined before tracking. Events are dropped as before and all
   values of tags that would exceed the limit are dropped.

#### Sinks

Vector has a number of sinks that can encode metrics. These will need to be audited to determine
which sinks are limited to single-valued tags and which can accept the full multi-valued tags. The
former will receive only the last value of any multi-valued tag, and will emit a new rate-limited
warning when doing so. The latter will be upgraded to encode multi-valued tags appropriately.

### Implementation

#### Representation

The tags representation will change from an alias into a newtype wrapper. This newtype will hide the
implementation details of the underlying storage from the callers.  This wrapper will use an
`indexmap` set to store the tag values. It will also add separate methods for inserting a new tag
and replacing a tag set with a single value. The callers of the existing `insert` function will need
to be audited to determine which use is intended at each call site. The tag values themselves are
stored as optional strings, in which the `None` value represents a bare tag.

```rust
type TagValue = Option<String>;

struct TagValueSet(indexmap::IndexSet<TagValue>);

struct MetricTags(BTreeMap<String, TagValueSet>);

impl MetricTags {
    /// Insert returns the value unchanged if the exact (tag,value) pair already exists,
    /// otherwise it inserts a new value for the named tag.
    fn insert(&mut self, name: String, value: TagValue) -> Option<TagValue>;

    /// Replace returns all the existing values when overwriting a tag.
    fn replace(&mut self, name: String, value: Option<TagValue>) -> Option<TagValueSet>;

    /// Replace an entire tag set with the given value set, returning existing values.
    fn replace_all(&mut self, name: String, values: TagValueSet) -> Option<TagValueSet>;

    /// Remove a single value of a tag.
    fn remove(&mut self, name: &str, value: Option<&str>) -> Option<TagValue>;

    /// Remove all values for a tag name.
    fn remove_all(&mut self, name: &str) -> Option<TagValueSet>;
}
```

#### Lua Type Conversion

Converting from Vector types to Lua, to be able to pass data into Lua functions, is controlled by
the `ToLua` trait. This trait works similar to the standard Rust `Into` and `From` traits. The
functions of these traits are passed a reference to the Lua interpreter object, of which we create a
unique instance for each transform. This interpreter object can store user-defined data via a set of
["app data"](https://docs.rs/mlua/latest/mlua/struct.Lua.html#method.set_app_data) interfaces. We
will use these interfaces to store the state of the `metric_tag_values` flag for the particular
transform, and use the value to inform which conversion mode to use on tags.

## Rationale

The guiding rationale for all of the external changes is to maximize backwards compatibility while
adding support for the new functionality. This means that, where possible, Vector should accept all
existing tag assignments as-is using current formats, understanding that output representations will
have to change to accomodate the new capabilities of the data set.

The proposed Protobuf representation allows all possible combination of values for a tag set, and
minimizes the encoded size in the presence of repeated tag names. It also requires no further
parsing to separate out tag names from values. Since the existing `tags` element is a strict
string-to-string map, we cannot enhance the value part of the map and so a new element is required.

Similarly, the proposed native JSON encoding adds the necessary support while preserving backwards
compatibility for existing data and minimizes the overhead when multiple tag values are not
required.

The multi-stage introduction of tag array values to the scripting environment follows the standard
Vector practice for introduction of breaking behavior changes: add the new behavior as an option but
default to the existing behavior, later add a deprecation warning; then change the default behavior
option, and finally remove the old behavior.

Changing the `MetricTags` type from an alias to a newtype wrapper allows us to provide better
compatibility for existing uses while controlling the methods for uses that need to access all the
values.

The use of an `IndexSet` for the tag value provides us with two useful invariants:

1. Only unique values for each tag will be stored, which prevents repeated values from showing up in
   the output.
1. The values can be retrieved in the order they first appeared, which allows us to trivially
   retrieve either the first or last stored value.

## Drawbacks

The data model changes may cause observable behavior changes in sinks in the presence of data that
now has multi-valued tags, ie when receiving data from sources that produce multi-valued tags.

There is no way to support multi-valued tags in Lua or VRL scripts without introducing breakage of
one form or another. Exposing tags as multi-valued will break scripts. The proposed scheme avoids
breaking existing scripts, but will cause Lua scripts in "single" mode to reduce all multi-valued
tags to a single value, even if there are no modifications to tags.

## Prior Art

The Datadog agent stores metric tags as a simple set of strings, equivalent to `HashSet<String>`. It
also does some clever hashing and deduplication internally to make this work efficiently. However,
the agent doesn't do anything more interesting with the tags than adding and removing whole strings,
which does not cover all our use cases.

## Alternatives

### External Representation

We could represent the tags in the Vector Protobuf definition as a simple array of strings, matching
the source Datadog agent data. Where there are no repeated tag names, this is also the most size
efficient representation. This, however, embeds the assumption that the separator is a particular
character (an ASCII colon in this case) that cannot be represented in the tag name. It also requires
parsing after the data is received to split the values into name-value pairs.

We could also change the native JSON encoding to unconditionally output arrays for all tag values in
order to simplify the encoding algorithm. However, given that we have to retain decoding for tags
without the array values, we can make use of this form to reduce the complexity of the encoded data.

### Scripting

There are a handful of alternative paths for adding support for our scripting languages, all of
which will cause problems for users:

1. Unconditionally expose the tags as arrays of values using the existing naming, but still accept
   assignments using either single values or arrays of values. This will cause breakage to existing
   scripts that relies on the existing single value tag values.
1. For Lua or VRL scripts, conditionally expose the tags as single values or arrays, as described in
   the proposal, but accept assignments following the native JSON codec scheme. In Lua, this could
   cause a breaking change where scripts that emit metrics that have the wrong tabs type to be
   accepted for transmission. In VRL, this would create headaches for type definitions, at best
   preventing proper validation of programs.
1. Expose the tags as single values using the existing naming, picking some arbitrary value when a
   tag has multiple values, and set up a secondary tags structure that exposes the arrays. This will
   lead to all kinds of confusion and conflicts when the same tag is assigned through different
   variables.
1. Add functions specifically for manipulating tag sets. This continues to make metrics management
   look like a second-class afterthought, and doesn't ease any compatibility problems for existing
   scripts.

### Internal Representation

The Datadog agent represents the tags as a simple set of strings, ie `HashSet<String>` or
`BTreeSet<String>`, where the key/value implications are just an interpretation detail for
consumers. This is by far the simplest possible storage. However, Vector needs to be able to access
tags by name for manipulation, making this representation more challenging. Through clever use of
`BTreeSet::range` and a wrapper type for the tags, this could be made to work, but it is unclear if
the benefits would be worth the additional complexity.

When retaining `BTreeMap` as the top-level container, there are a number of options for the value
that would support this feature but with different semantics:

1. `Vec<String>` — Retains the ordering of tags as they appear, but allows for duplicate values and
   cannot support both bare tags and multiple values simultaneously.
1. `Vec<Option<String>>` — Same as above but supports bare tags and mutiple values simultaneously.
1. `BTreeSet<Option<String>>` — Duplicate values are merged but are sorted, likely putting the bare
   tag first for single-value uses.

There are also at least two other container types that could possibly support this use case:

[multimap](https://docs.rs/multimap/latest/multimap/) is a wrapper around a `HashMap` with a `Vec`
in the value position. However, it does not sort the keys on retrieval, which changes the ordering
guarantee that `BTreeMap` is currently providing.

```rust
type MetricTags = multimap::MultiMap<String, Option<String>>;
```

[multi_index_map](https://crates.io/crates/multi_index_map) supports both ordering of the tag names
and multiple values. It is not clear, however, if there is any way to control for unique values for
the same name without also preventing the same value appearing in different tags, nor how those
values would be ordered.

```rust
#[derive(MultiIndexMap, Clone, Debug)]
struct Tag {
    #[multi_index(ordered_non_unique)]
    name: String,
    value: Option<String>,
}

type MetricTags = MultiIndexTagMap;
```

## Outstanding Questions

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [X] Convert the `MetricTags` alias to a newtype wrapper
- [ ] Convert the `MetricTags` type to new storage as above but expose as single tags
- [ ] Introduce insert/replace distinction and audit all uses
- [ ] Update the native protobuf encoding
- [ ] Update the native JSON encoding
- [ ] Update the `lua` transform to support multi-valued tags
- [ ] Update VRL to support multi-valued tags
- [ ] Update metric sources that could receive multi-valued tags
- [ ] Update metric sinks to emit multi-valued tags
- [ ] Add multi-valued tag support to the `log_to_metric` transform
- [ ] Update the `tag_cardinality_limit` transform for multi-valued tags
- [ ] Add deprecation warnings for single-valued tags behavior (Lua and VRL)
- [ ] Change default behavior to multi-valued tags (Lua and VRL)
- [ ] Drop single-valued tags support (Lua and VRL)

## Future Improvements

Most often, tags will only have a single value instead of an array. This suggests that an
implementation that stores tag values as an enum switching between a simple `TagValue` and the above
`IndexMap` would be more efficient for memory consumption and likely be more efficient for CPU time.

Since tags will most often only have a single value, we may not want to change the default behavior
of Lua and VRL to use multi-valued tags, as it represents a regression in user experience, and so
also not remove the single value support. This will be determined after we start the deprecation
process.

We could also investigate reworking the storage based on `BTreeMap<String>`. This would allow us to
avoid splitting tag strings in two pieces, reducing allocations and overhead, at the cost of
increased complexity when accessing a particular named tag.

Metric tag sets are most often repeated a great number of times across different metrics. This
suggests that a shared copy-on-write storage scheme where the individual metrics would contain just
a handle to the shared value. This would improve Vector's memory efficience at least, and possibly
run-time performance as well.

The schema definitions that are already present on sinks could be enhanced to add support for what
types of tags sources and sinks support. This would be used in any intermediate scripting transform
(Lua or VRL) to set the tag mode exposed by that transform, instead of requiring users to manually
set `metric_tag_values` appropriately for their topology. For VRL this would have the benefit of
turning the run-time problem of overwriting a multi-valued tag with a single value into a
startup-time error, thus preventing data loss.

It may be useful to add functions to VRL to improve the ergonomics of manipulating tag arrays. In
particular, removing a tag value from a set of tags, if present, is awkward at best without a
helper. The best form of such helpers is best left for future work based on how such tags are
actually used.
