---
title: Dedupe events
short: Dedupe
kind: transform
---

## Configuration

{{< component/config >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### Cache behavior

This transform is backed by an LRU cache of size [`cache.num_events`](#num_events). That means that this transform caches information in memory for the last `cache.num_events` that it has processed. Entries are removed from the cache in the order in which they were inserted. If an event is received that's considered a duplicate of an event already in the cache, that will put that event back to the head of the cache and reset its place in line, making it once again last entry in line to be evicted.

### Memory usage details

Each entry in the cache corresponds to an incoming Event and contains a copy of the "value" data for all fields in the Event being considered for matching. When using `fields.match` this will be the list of fields specified in that configuration option. When using `fields.ignore` that will include all fields present in the incoming event except those specified in `fields.ignore`. Each entry also uses a single byte per field to store the type information of that field. When using `fields.ignore` each cache entry additionally stores a copy of each field name being considered for matching. When using `fields.match` storing the field names is not necessary.

### Memory utilization estimation

If you want to estimate the memory requirements of this transform for your dataset, you can do so with these formulae:

When using `fields.match`:

```
Sum(the average size of the *data* (but not including the field name) for each field in `fields.match`) * `cache.num_events`
```

When using `fields.ignore`:

```
(Sum(the average size of each incoming Event) - (the average size of the field name *and* value for each field in `fields.ignore`)) * `cache.num_events`
```

### Missing fields

Fields with explicit null values will always be considered different than if that field was omitted entirely. For example, if you run this transform with `fields.match = ["a"]`, the event `"{a: null, b:5}"` will be considered different from the event `"{b:5}"`.

### State

{{< snippet "stateless" >}}
