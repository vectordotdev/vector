# RFC 14742 - Support for Duplicate and Bare Tags on Metrics

Vector's current metric model supports only a single value for each tag. The `datadog_agent` source,
however, may send metrics that contain multiple values for a given tag name, or bare tags with no
value. Vectors tag support should be enhanced to handle this data.

## Context

- [Support duplicate tag and bare tags on metrics](https://github.com/vectordotdev/vector/issues/14742)
- [DataDog Source -> Filter Transform -> DataDog Sink dropping kube_service tag](https://github.com/vectordotdev/vector/issues/14707)
- [Invalid custom_tags metrics from sinks datadog_metric](https://github.com/vectordotdev/vector/issues/14239)

## Scope

### In scope

- Changes to the existing metric tag internal and external representation to support multiple values
  and bare tags.

### Out of scope

- Changes to any other aspect of metric or event representations.

## Pain

When ingesting data from a Datadog agent, the metric tags provided by that source may contain data
that Vector's data model cannot represent. This is causing problems for prospective users that want
to add Vector to their observability pipelines but cannot do so because it would cause data loss.

## Proposal

### User Experience

This change should be effectively invisible to users of Vector, except for incoming data with
multiple tags with the same name, which will now have all the tags reproduced when it reaches sinks.

### Implementation

#### Internal Representation

Simply put, the tags representation will change from an alias into a newtype wrapper using an
`indexmap` set to store the tag values. This newtype will hide the implementation details of the
underlying storage from the callers. It will also add separate methods for inserting a new tag and
replacing a tag set with a single value. The callers of the existing `insert` function will need to
be audited to determine which use is intended at each call site. The tag values themselves are
stored as optional strings, in which the `None` value represents a bare tag.

```rust
type TagValue = Option<String>;

struct MetricTags(BTreeMap<String, indexmap::IndexSet<TagValue>>);

impl MetricTags {
    // Insert returns the value unchanged if the exact (tag,value) pair already exists,
    // otherwise it inserts a new value for the named tag.
    fn insert(&mut self, name: String, value: TagValue) -> Option<TagValue>;

    // Replace returns all the existing values when overwriting a tag.
    fn replace(&mut self, name: String, value: Option<TagValue>) -> Option<IndexSet<TagValue>>;
}
```

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

## Rationale

Changing the `MetricTags` type from an alias to a newtype wrapper allows us to provide better
compatibility for existing uses while controlling the methods for uses that need to access all the
values.

The use of an `IndexSet` for the tag value provides us with two useful invariants:

1. Only unique values for each tag will be stored, which prevents repeated values from showing up in
   the output.
1. The values can be retrieved in the order they first appeared, which allows us to trivially
   retrieve either the first or last stored value.

The proposed Protobuf representation allows all possible combination of values for a tag set, and
minimizes the encoded size in the presence of repeated tag names. It also requires no further
parsing to separate out tag names from values.

## Drawbacks

Metrics sinks that only support a single value per tag will need to be reworked accordingly for the
new value type. Additionally, these sinks may change their behavior for sources that have been
producing multiple tag values.

## Prior Art

The Datadog agent stores metric tags as a simple set of strings, equivalent to `HashSet<String>`. It
does some clever hashing and deduplication internally to make this work efficiently. However, it
doesn't do anything more interesting with the tags than adding and removing whole strings, which
does not cover all our use cases.

## Alternatives

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

### External Representation

Similar to the above alternatives, we could represent the tags in the Vector Protobuf definition as
a simple array of strings, matching the source Datadog agent data. Where there are no repeated tag
names, this is also the most size efficient representation. This, however, embeds the assumption
that the separator is a particular character (an ASCII colon in this case) that cannot be
represented in the tag name. It also requires parsing after the data is received to split the values
into name-value pairs.

## Outstanding Questions

- VRL doesn't have any way of _adding_ a tag to a metric when a tag with the same name already
  exists, only replacing or deleting, nor does it really have support for creating bare tags nor
  retrieving all the values of a tag name. Does it need additional functions to match this support?

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Convert the `MetricTags` alias to a newtype wrapper
- [ ] Convert the `MetricTags` type to new storage as above
- [ ] Audit all uses of `MetricTags::insert` to see which should do a replace

## Future Improvements

As referenced above, once the initial implementation is complete, we could rework the storage based
on `BTreeMap<String>`. This would allow us to avoid splitting tag strings in two pieces, reducing
allocations and overhead, at the cost of increased complexity when accessing a particular named tag.

Metric tag sets are most often repeated a great number of times across different metrics. This
suggests that a shared copy-on-write storage scheme where the individual metrics would contain just
a handle to the shared value. This would improve Vector's memory efficience at least, and possibly
run-time performance as well.
