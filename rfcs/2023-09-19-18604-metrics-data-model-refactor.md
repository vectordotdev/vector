# RFC 18604 - 2023-09-18 - Metrics data model refactor

Vector's data model for metrics incorporates a large number of possible metric types that it can
natively handle, along with a variety of information/metadata about those metrics. Over time, easily
(and correctly) handling all of those metric types within each metrics-specific source and sink
component has been difficult, consuming both a large amount of developer time as well as leading to
subtle and hard-to-diagnose issues when shuttling data from one metrics system/encoding to another.
This RFC proposes a body of refactoring work meant to addresses current issues with the data model,
as well as help future-proof it.

## Context

- [Support AggregatedSummary for `gcp_stackdriver` sink](https://github.com/vectordotdev/vector/issues/9530)
- [Support aggregated histograms in `statsd` sink](https://github.com/vectordotdev/vector/issues/11661)
- [Prometheus remote write source has incomplete support/behavior.](https://github.com/vectordotdev/vector/issues/10801)
- [Cumulative histogram in aggregation transform](https://github.com/vectordotdev/vector/issues/12745)
- [Support Prometheus native histograms](https://github.com/vectordotdev/vector/issues/16333)
- [Support converting Histogram to Summary](https://github.com/vectordotdev/vector/issues/13754)

## Cross-cutting concerns

- Stability / schema guarantees around `native_json` codec by basing it on the Protocol Buffers
  schema (no overarching Github issue for this yet) and how changes proposed by this RFC would alter
  the output of that codec

## Scope

### In scope

- Adding/removing metric types to the data model.
- Updating metrics-capable components to handle changes to the metrics data model.

### Out of scope

- Bolstering supporting for handling/manipulating metrics in a pipeline (e.g. enhancing metrics
  support in the `lua` transform).
- Explicitly adding support for other metrics systems (e.g. OpenTelemetry).

## Pain

While the current metrics data model is, on the surface, scoped well to the metrics systems we
support, its specificity often leads to issues with transforming metrics from one metrics to
another.

### Some metric types are specific to a single metrics system

An example of this is Prometheus and Vector's native support for aggregated histograms and
summaries. In Prometheus, aggregated histograms and summaries represent a point-in-time snapshot of
a distribution of values, encoded into either buckets or summarized by the probability distribution.
Both of these data types, when shown in a human-readable form, are essentially a group of gauges
where some gauges represent summary statistics (total sum of values, total count of values) and the
rest represent the individual buckets or quantiles of the distribution.

While these data types allow us to easily ingest Prometheus data, as well as emit it, problems arise
when transforming these metrics to other metrics systems. In particular, many metrics systems have
no concept at all of an "aggregated histogram". Perhaps the system only supports counters, gauges,
and histograms (but aggregated server-side, so Vector only sends the raw samples) and so the
aggregated metrics have to be decomposed... or perhaps the aggregated histograms can be transformed
to something like [DDSketch](https://www.vldb.org/pvldb/vol12/p2195-masson.pdf). Whatever the
mechanism, it's an additional metric type that must be supported individually for all metrics sinks.

### Needless differentiation within metric types

An example of this is the inherent "statistics" for a distribution metric.

In practice, this leads to Vector users being forced to choose/consider the difference between a
"histogram" and a "summary", when internally, there is no actual difference: a distribution carries
raw sample values, and merely calculate summary statistics over those sample values. For metric
sinks, those summary statistics are never used, as we simply deal with the raw sample values.

Beyond the user experience pain, this is extra code that needs to be handled/dealt with internally,
even when (again) it has no inherent value.

## Proposal

We would make the following changes to the metrics data model:

- remove `MetricValue::AggregatedSummary` and `MetricValue::Set` entirely
- refactor `MetricValue::Distribution` into an enum with a new `Raw` variant
- merge `MetricValue::AggregatedHistogram` into `MetricValue::Distribution` as `FixedHistogram`
- merge `MetricValue::Sketch` into `MetricValue::Distribution`

For a more visual representation, we would refactor `MetricValue` from this:

```rust
pub enum MetricValue {
    Counter { value: f64 },
    Gauge { value: f64 },
    Set { values: BTreeSet<String> },
    Distribution { samples: Vec<Sample>, statistic: StatisticKind },
    AggregatedHistogram { buckets: Vec<Bucket>, count: u64, sum: f64 },
    AggregatedSummary { quantiles: Vec<Quantile>, count: u64, sum: f64 },
    Sketch { sketch: MetricSketch },
}
```

to this:

```rust
pub enum Distribution {
    Raw { samples: Vec<Sample> },
    Histogram { buckets: Vec<Bucket>, count: u64, sum: f64 },
    Sketch { sketch: MetricSketch },
}

pub enum MetricValue {
    Counter { value: f64 },
    Gauge { value: f64 },
    Distribution { distribution: Distribution },
}
```

### Removal of `AggregatedSummary`

We would remove `AggregatedSummary` entirely, opting to handle them by decomposing them into
individual metrics when received, and reassembling them (if need be) back into a single metric when
shipping them out.

Users would no longer be able to create metrics of this type in the `lua` transform.

### Removal of `Set`

We would remove `Set` entirely, opting to handle the semantics of sets directly in the sources that
support generating set metrics.

Users would no longer be able to create metrics of this type in the `lua` transform. There would
additionally be a change to how set metrics were handled by Vector due to changes in how they would
be aggregated on ingestion.

### Refactoring `Distribution` into an enum that handled all distribution types

We would refactor `Distribution` to become an enum that handled raw values (as it does today) as
well as histograms and sketches, which would involve merging both `AggregatedHistogram` and `Sketch`
into `Distribution`, effectively removing those two metric types. We would additionally remove the
"statistics" currently bundled into `Distribution`.

The user-visible changes would be:

- no more `summary` metric type when using the `log_to_metric` transform, and `histogram` would be
  renamed to `distribution` (would map to `Distribution::Raw`, which would maintain the behavior of
  specifying `histogram`/`summary` in terms of storing the raw sample)
- no more `AggregatedHistogram` metric type when using the `lua` transform, as it would be renamed
  to `histogram`

## Implementation

### Removal of `AggregatedSummary`

We would entirely remove the `AggregatedSummary` variant from `MetricValue`. In order to still be
able to handle aggregated summaries coming in and going out via the various Prometheus-specific
sources and sinks, we would take an approach of separating (and combining) them into their
individual values.

For example, an aggregated summary is represented as the total count of samples, the total sum of
the samples, and a count of samples within a given quantile. For the count, sum, and each quantile's
count, we can represent those values as individual gauges. For count and sum, this is
straightforward, and merely requires appending a suffix to the metric name to individual which value
is which. For each quantile's count, a tag would be added that mimics the tagging structure used by
Prometheus itself, such that the tag value is the bucket's upper bound.

#### Prometheus-specific sources

For Prometheus-specific sources, aggregated summaries would be separated based on the logic that is
otherwise used to emit them in a sink. As mentioned above, for ingesting these aggregated summaries,
we would emit as multiple metrics: one for count, one for sum, and one for each bucket or quantile.
For the bucket or quantile metrics, they would have the relevant tag that would be used if emitting
the original aggregated metric in the Prometheus exposition format.

#### Prometheus-specific sinks

For Prometheus-specific sinks, aggregated summaries would be recomposed/grouped based on both their
series name as well as tags.

In the Prometheus exposition format, an aggregated summary might have a series name of `foo`. For
both the count and sum of that aggregated metric, the series name for those values would be
`foo_count` and `foo`. For the quantiles, multiple `foo` series would be emitted, each with the tag
for the quantile, such as `foo{quantile="0.99"}`.

In this way, the logic to group these decomposed metrics together is primarily based on prefix, but
there is no specific marker to know if an individual metric itself should trigger such grouping
logic. One approach could be to check each metric being processed to see if it was named in such a
way as to represent the count or sum of an aggregated summary, or if it contained a tag that would
represent a quantile. For metrics that matched this heuristic, they could be stored off to the side
temporarily based on their "true" series name, and other metrics could then be processed, which
would either lead to the remaining individual metrics being recombined or not. For any metrics
falsely captured by this heuristic, they would simply be handled as-is at the end of processing.

### Removal of `Set`

We would entirely remove the `Set` variant from `MetricValue`. In order to still be able to handle
sets coming in, we would handle aggregation at the source itself.

In StatsD, the original model for all metric types was server-local aggregation: the `statsd` daemon
that you sent metrics to would handle the aggregation, and on a configurable interval, emit the
aggregated value. This applied to sets as well: the count of unique values in a set, during the
given aggregation window, was emitted as a gauge. Similarly, the Datadog Agent works this way as
well, emitting a gauge representing the counter of unique values seen during the aggregation window.

Currently, Vector will handle sets natively, passing along the actual value seen up until it reaches
a sink, where we either emit the metric directly (in the case of the `statsd` sink) or convert it to
a gauge, such as in the `datadog_metrics` sink.

With this change, we would move to aggregating set values within the `statsd` source itself and
emitting them, as a gauge, on an interval. This would accomplish a few things:

- we would remove the need to handle `Set` metrics in all metrics-capable components knowing that,
  in nearly all cases, they will simply be turned into a gauge before leaving Vector
- we would emulate StatsD/DogStatsD in terms of aggregation behavior, emitting the gauge directly in
  the source, which would maintain parity with how StatsD/DogStatsD emit any set metrics

This is not without potential caveats, namely around metric tagging and how those emitted gauges can
be aggregated downstream (more on that in the Drawbacks section)

### Refactoring `Distribution` into an enum that handled all distribution types

We would refactor the `Distribution` variant to be a dedicated enum that represented all possible
forms of a "distribution". While there are a few different forms and usaes for the word
"distribution," we're referring to them here as any metric that encapsulates multiple
samples/observations. This could be the raw samples themselves, or it could be a
compressed/condensed form such as a histogram or sketch.

One key aspect here is that a distribution to meant to encompass all relevant/possible forms that
Vector may need to handle and in a way that can be used interchangably between sources and sinks,
such as taking in a fixed-bucket histogram from the `prometheus_scrape` source and being able to get
a DDSketch to send out from the `datadog_metrics` sink.  Said another way, the goal here is to be
able to hand a `Distribution` metric to any metrics-capable sink and allow it to represent that
distribution in the highest fidelity way possible, whether that's converting from one distribution
variant to another, or downsampling it such as emitting fixed quantiles from a sketch, and so on.

#### Raw distribution

The `Raw` variant would match the current shape of `Distribution` where the raw samples are stored, which
includes the sample value and the sample rate. This variant is used for sources where the sample
value comes in directly (such as StatsD histograms) or use cases like the `log_to_metric` transform,
where the value is also coming directly from a log event.

#### Histograms

The `Histogram` variant would be the spiritual successor to `AggregatedHistogram`, where we have a true
histogram (bucketed values) but the number of buckets, and the bucket sizes, are fixed. While a
DDSketch can be decomposed into its individual buckets, such that we could theoretically use a
DDSketch to emit a Prometheus-capable aggregated histogram, we have no mechanism to store the
original buckets used from the source metric within a DDSketch. Since Prometheus' own aggregation
depends on having equivalent buckets for aggregated histograms, we're providing this variant to
handle that use case.

All of that said, this variant can still be remapped into a DDSketch (via interpolation) and so it
is otherwise compatible with the goal of a `Distribution` being able to represent itself in multiple
forms while still allowing for conversion to the highest fidelity represent for a given consumer of
that distribution metric.

#### Sketches

This variant would be the merged version of the original `Sketch` variant, which currently only
contains the `DDSketch` variant, mapped to the DDSketch configuration parameters used by the Datadog
Agent.

Similarly to aggregated histograms, this variant specifically supports the Datadog Agent use case,
which sends distributions as a compressed/condensed data structure called `DDSketch` that allows for
summarizing the distribution data in a space-efficient way while still providing a bound on the
error of values estimated by the sketch.

While we could additionally refactor this to only ever refer to the agent-specific configuration of
DDSketch -- as we do not currently support any other sketch implementations/configurations of
DDSketch -- we're leaving this as-is because the current sketch code is fairly complex and is likely
subject to change in the future. Simplification can be done at that time, and the enum-based
approach currently used for the particular sketch variant provides us an escape hatch for that
future refactoring work.

### Backwards compatibility in Vector's Protocol Buffers definition

Another higher-level concern is the backwards compatibility in Vector's Protocol Buffers schema,
namely the schema used for intra-Vector communication (`vector` source and sink) and disk buffering.

In order to handle this for ensuring a seamless transition, we would specifically update the logic
used to convert the Protocol Buffers representation of events into the native Vector representation,
such that we followed the same logic as described above. We would ensure that Vector itself no
longer emits metric types that have been removed, and that it emits the updated representation for
any refactored metric types. This would allow us to narrowly focus compatibility efforts on the
conversion _from_ Protocol Buffers, rather than also having deal with the other direction.

For example, the logic used to decompose an aggregated summary into individual metrics would also be
used in the code that converts metrics in their Protocol Buffers representation to the native Vector
representation. For aggregated histograms, we would simply emit a distribution of the "fixed
histogram" variant, and so on.

## Rationale

### Simplified data model

Our data model has existed in its current state for many years, and over time, the niche metric
types that we support cause frequent toil when working in areas of the code that deal with metrics.
As an example, every metrics sink must handle aggregated histograms and summaries, which they
generally go about one of two ways: either breaking each bucket/statistic into an individual gauge
(as this RFC proposes) or by converting it into another statistical data structure, such as
`DDSketch`.

By simplifying the data model, we can remove nearly all of that boilerplate handling code and simply
let sinks handle each metric type in a more direct way. For counters and gauges, this is already
taken care of as it stands today. For sinks handling either aggregated histograms, distributions, or
sketches, they would now only need to deal with `Distribution`. Helper methods will be provided to
convert `Distribution` into the highest fidelity form supported by the sink: a sink that supports
sketches can ask for it to be converted to a sketch, while another sink that only supports
histograms can ask for a histogram, since all distribution variants can be converted to either a
histogram or sketch.
### Aligns more closely with other metrics systems

While the current data model is more of a union of possible metric types based on the metric systems
we can ingest from, the observability world has been continually shifting over time to a simpler and
more unified data model that simply deals with counters, gauges, and histograms. This can be seen in
observability systems such as OpenTelemetry, or observed directly, such as by examining the allowed
metric types for something like Datadog's ingestion APIs.

While SDKs and helper code may provide higher-level primitives on top of these, such as rates and
meters, we can simplify our underlying data model both for our own benefit, as well as the benefit
of interoperability.

## Drawbacks

### Reassembling decomposed metrics for correct handling

The main drawback would be the complexity required to handle the "recomposing" of individual metrics
where an aggregated summary could otherwise be handled.

For Prometheus-specific sinks, this would entail grouping the metrics together. This is trivial to
do in the naive sense of, say, sorting them so that they're all adjacent in the sink output.
However, the trickier part would be the logic to essentially "find" them and determine that they're
the decomposed pieces of an aggregated histogram, rather than simply uncorrelated metrics. This is
import for the ability to properly tag the metrics as being part of an aggregated histogram or
summary.

In general, there are other facets that would add risk to this recomposing logic, such as mutation
of the metrics in a topology (tag governance/cost control modifications leading to missing
individual metrics) or batching in time and/or space that leads to metrics losing their natural
grouping.

Some of these risks are greater than others: tag governance/cost control techniques in topologies
are very common, but events being split out of their original batch is very rare. However, the risks
are still real and ultimately depend on the specifics of a given configuration, or could be
influenced by unrelated changes to Vector internals.

### Large number of distribution types

While the goal of the RFC is ostensibly to simplify the metrics data model, we do still have an
outsized number of "distribution" variants compared to other metric types like counters and gauges.

This is an unfortunate reality due to the nature of how different metrics systems represent their
distribution/histogram types, and the fundamental limitations on having one type that can represent
all of them without losing fidelity.

### Additional complexity in the Protocol Buffers schema

This proposed set of changes would add additional metric type message definitions to the
already-large set of definitions specific to metrics in Vector's Protocol Buffers schema. Beyond
that, it will also require additional logic in the Protocol Buffers/Vector representation conversion
code to handle providing the same metric decomposition and so on, which is further technical debt in
that area of the code, which already handles many of the aforementioned message definitions that are
now deprecated.

## Alternatives

### Following the OTLP metrics data model exactly

One alternative approach would be to follow the OpenTelemetry metrics data model exactly. This
approach would technically allow us to support all existing metric types directly, with an option
for still removing some metric types if we so desired.

In essence, we would change Vector's `MetricValue`, and its variants, to match the metric types
supported by OpenTelemetry, and update metrics-capable sources/transforms/sinks to align with the
new metric types.

This option is potentially attractive as it aligns Vector more closely with OpenTelemetry, which is
useful from an interoperability standpoint. However, this option (at least insofar as the option
described in the context of this RFC) does not cover the additional changes that would be required
to fully align Vector with OpenTelemetry, such as matching terminology (e.g. OTLP uses "aggregation
temporality" where Vector uses "metric kind") or maintaining additional metadata, like if a metric
was a floating-point or integer value, or handling exemplars.

That said, it's not clear that there's enough value to doing this given that the RFC prosposal
itself is still fully compatible with OpenTelemetry, but simply lacks alignment in areas like
terminology, etc.

#### Mapping counters, gauges, and aggregated histograms

Counters ("sums" in OTLP) and gauges are natively supported, so we would not need to do anything
extra to map these metric types to OTLP.

Aggregated histograms are also natively supported, but they're simply called histograms in OTLP.

#### Mapping sets

Sets have no natural equivalent in OTLP. We would either need to drop support for them entirely, or
follow the described path in the RFC of aggregating them at source level and emitting gauges
directly.

#### Mapping aggregated summaries

Technically, the OpenTelemetry metrics data model
[does support summaries](https://opentelemetry.io/docs/specs/otel/metrics/data-model/#summary-legacy),
but they are deprecated and not encouraged to be used, given that they lack the natural ability to
be aggregated server-side.

This is an area where we could either continue to support them natively in Vector -- with all of the
logic we currently have to convert them to individual gauges where not supported by sinks -- or
simply do the work to decompose them at the source-level and remove them as a metric type.

#### Mapping distributions

For the current `Distribution` metric type, we would map it to an OTLP `ExponentialHistogram`. This
is the highest fidelity option for representing a wide range of sample values in a way that is
mergeable.

#### Mapping sketches

Sketches have no natural equivalent in OTLP, at least as Vector supports them, which is only through
a Datadog Agent-specific implementation of `DDSketch`. However, `DDSketch` itself is essentially a
histogram under-the-hood, so we would be able to represent it as an OTLP histogram by simply
exposing all sketch buckets directly.

This would preserve the error guarantees of `DDSketch` itself as we would simply be representing it
directly in terms of the buckets, but we would lose the useful properties such as being able to only
track buckets that have values in them. While it might seem like a natural fit to map the `DDSketch`
data structure to OTLP's exponential histogram, which functions the same in principle, a
discontinuity between how both of them choose their bucket index mappings leaves Vector unable to
losslessly convert one to the other.

## Outstanding Questions

- While we ostensibly look to fully support all systems that Vector provides components for, set
  usage in StatsD is anecdotally almost non-existent. How strong of an indicator would we want to
  see before feeling comfortable removing support for sets entirely? This would imply not
  aggregating them in the `statsd` source at all, as we would still remove `Set` from `MetricValue`.
- Would we want to go as far as marking the removed/refactored metric types as deprecated, and then
  remove support for them entirely from the Protocol Buffers schema in a future release? The
  conversion logic (for going from the Protocol Buffers representation to the native Vector
  representation) would represent somewhat of a technical debt footgun if we did further
  refactoring in the future.

## Plan Of Attack

- [ ] Update the Protocol Buffers/Vector conversion code to handle the decomposing/recomposing of
  aggregated summaries
- [ ] Update the Prometheus sources/sinks to handle the decomposing/recomposing of aggregated
  summaries
- [ ] Remove `AggregatedSummary` from `MetricValue`
- [ ] Add support to `statsd` source to aggregate sets directly (still an outstanding question that
  would change whether or not this happens)
- [ ] Remove `Set` from `MetricValue`
- [ ] Update the Protocol Buffers/Vector conversion code to handle the decomposing of aggregated
  histograms
- [ ] Refactor `Distribution` in `MetricValue` to subsume `AggregatedHistogram` and `Sketch`
  (refactoring of `MetricValue`, updating components to use new variant/subvariants, updating
  Protocol Buffers/Vector conversion code)
