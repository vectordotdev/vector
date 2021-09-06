# RFC 8547 - 2021-09-01 - Accept metrics in `datadog_agent` source

Currently the `datadog_agent` [source](https://vector.dev/docs/reference/configuration/sources/datadog_agent/) only
supports logs. This RFC suggests to extend Vector to support receiving metrics from Datadog agents and ingest those as
metrics from a Vector perspective so they can be benefit from Vector capabilities.

## Context
- Vector is foreseen as a Datadog Agents aggregator, thus receiving metrics from Datadog Agents is a logical development
- Vector has support to send metrics to Datadog, thus receiving metrics from Agent is a consistent feature to add

## Cross cutting concerns

None so far.

## Scope

### In scope

- Implement a Datadog metrics endpoint in Vector, it will match the [metrics intake
  API](https://docs.datadoghq.com/api/latest/metrics/) with additional route that the Agent uses
- Include support for sketches that uses protobuf
- Ensure all Datadog metrics type are mapped to internal Vector metric type and that there is no loss
- TBC if needed: internal Vector adjustement, probably required for sketches

### Out of scope

- Anything not related to metrics
  - API validation requests
  - Other kind of payloads: traces, event, etc.

## Pain

- Vector cannot fully aggregate Datadog Agent traffic
- Inconsistency between Datadog logs vs. metrics support in Vector

## Proposal

### User Experience

- Vector will support receiving Datadog Metrics sent by the official Datadog Agent through a standard source
- Metrics received will be fully supported inside Vector, all metric types will be supported
- The following metrics flow: `n*(Datadog Agents) -> Vector -> Datadog` should just work
- No foreseen backward compatibily issue (tags management may be a bit bothersome)
- New configuration settings should be consistent with existing ones

### Implementation

A few details about the Datadog Agents & [Datadog metrics](https://docs.datadoghq.com/metrics/types/):
- The base structure for all metrics is named
  [`MetricSample`](https://github.com/DataDog/datadog-agent/blob/main/pkg/metrics/metric_sample.go#L81-L94) and can be
  of [several types](https://github.com/DataDog/datadog-agent/blob/main/pkg/metrics/metric_sample.go#L20-L31)
- Major Agent usecases:
  - Metrics are send from corechecks (i.e. go code)
    [here](https://github.com/DataDog/datadog-agent/blob/main/pkg/aggregator/sender.go#L227-L252)
  - Dogstatsd metrics are converted to the `MetricSample` structure
    [here](https://github.com/DataDog/datadog-agent/blob/main/pkg/dogstatsd/enrich.go#L87-L137) However Datadog Agents
    metrics are transformed before being sent, ultimately metrics accounts for two differents kind of payload:
- The count, gauge and rate series kind of payload, sent to `/api/v1/series` using the [JSON schema officially
  documented](https://docs.datadoghq.com/api/latest/metrics) with few undocumented [additional
  fields](https://github.com/DataDog/datadog-agent/blob/main/pkg/metrics/series.go#L45-L57), but this align very well
  with the existing `datadog_metrics` sinks.
- The sketches kind of payload, sent to `/api/beta/sketches` and serialized as protobuf as shown
  [here](https://github.com/DataDog/datadog-agent/blob/main/pkg/serializer/serializer.go#L315-L338) (it ultimately lands
  [here](https://github.com/DataDog/datadog-agent/blob/main/pkg/metrics/sketch_series.go#L103-L269)). Public `.proto`
  definition can be found
  [here](https://github.com/DataDog/agent-payload/blob/master/proto/metrics/agent_payload.proto#L47-L81).

Vector has a nice description of its [metrics data
model](https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/) and a [concise enum for
representing it](https://github.com/timberio/vector/blob/master/lib/vector-core/src/event/metric.rs#L135-L169).


The implementation would the consist in:

- Add route for metrics in the existing `datadog_agent` source (based on both the offical
  [API](https://docs.datadoghq.com/api/latest/metrics/) and the [Datadog Agent
  itself](https://github.com/DataDog/datadog-agent/blob/main/pkg/forwarder/telemetry.go#L20-L31)) to cover every metric
  type handled by this endpoint (count, gauge and rate) and
    - Add support for missing fields in the `datadog_metrics` sinks
    - Align tag manipulation (Datadog allows `key:foo` & `key:bar`, and the tags are encoded as a list whereas Vector
      encode those as a map).
    - Overall this is fairly straighforward except for the tagging issue
- Add another route in the `datadog_agent` source to support sketches/distribution encoded using protobuf
   - Revamp the distribution metrics in the `datadog_metrics` sink to use sketches and the associated endpoint will
     probably be a good idea.
   - The sketches the agent ships is based on this [paper](http://www.vldb.org/pvldb/vol12/p2195-masson.pdf) whereas
     Vector uses what's called a summary inside the Agent, implementing the complete DDSketch support in Vector is
     probably a good idea as sketches have convenient properties for wide consistent aggregation and limited error. To
     support smooth migration, full DDsktech support is mandatory, as customers that emit distribution metric from
     Datadog Agent would need it to migrate to Vector aggegation.

**Regarding the tagging issue:** A -possibly temporary- work-around would be to store incoming tags with the complete "key:value" string as the key and an empty value to store those in the extisting map Vector uses to store [tags](https://github.com/timberio/vector/blob/master/lib/vector-core/src/event/metric.rs#L60) and slightly rework the `datadog_metrics` sink not to append `:` if a tag key has the empty string as the corresponding value.

## Rationale

- Smoother Vector integration with Datadog.
- Needed for Vector to act as a complete Datadog Agent aggregator (but further work will still be required).
- Extend the Vector ecosystem, bring additional feature for distribution metrics that would enable consistent
  aggregation.

## Drawbacks

- It arbitrary imposes internal sketches representation to follow the DDSketch paper.

## Prior Art

- Existing Vector metrics source over HTTP(s) works well, those constructs will be reused
- Datadog logs is already supported in the `datadog_agent` source
- Vector has its own vector-to-vector protocol that serve a similar purpose but the idea is to enable Vector to receive
  Datadog metrics from unmodified Datadog Agents so that is probably not an option
- There are many [ways](https://edoliberty.github.io/papers/streamingQuantiles.pdf) of implementing quantile sketches

## Alternatives

- Use an alternate protocol between Datadog Agents and Vector (Like Prometheus, Statds or Vector own protocol)
- For sketches, we could flatten sketches and compute usual derived metrics (min/max/average/count/some percentile) and send those as gauge/count, but it would prevent (or at least impact) existing distribution/sketches user

## Outstanding Questions

- Datadog Agent configuration, diverting metrics to a custom endpoints will likely divert other kind of traffic, this
  need to be clearly identified / discussed
- Is there any other metrics type that could required some degree of adaptation, there is the non monotonic counter that exists in the agent but not in Vector that may require the introduction of a so-called `NonMonotonicCounter` in Vector, but

## Plan Of Attack

- [ ] Support the publicly documented route to the `datadog_agent` source for metrics type compatible with Vector, implement complete support in the `datadog_metrics` sinks for the undocumented fields, incoming tags would be stored as key only with an empty string for their value inside Vector. Validate the `Agent->Vector->Datadog` for gauge, count & rate.
- [ ] Extend Vector to support sketches following the DDSketch paper, this would be an extension
- [ ] Support payloads (**must be clearly identified**) that are sent with metrics by the Agent

## Future Improvements

- Wider use of sketches for distribution aggregation
- Continue on receiving other kind of Datadog payloads