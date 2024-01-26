# RFC 8547 - 2021-09-01 - Accept metrics in `datadog_agent` source

Currently the `datadog_agent` [source](https://vector.dev/docs/reference/configuration/sources/datadog_agent/) only
supports logs. This RFC suggests to extend Vector to support receiving metrics from Datadog agents and ingest those as
metrics from a Vector perspective so they can be benefit from Vector capabilities.

* [Context](#context)
* [Cross cutting concerns](#cross-cutting-concerns)
* [Scope](#scope)
  * [In scope](#in-scope)
  * [Out of scope](#out-of-scope)
* [Pain](#pain)
* [Proposal](#proposal)
  * [User Experience](#user-experience)
  * [Implementation](#implementation)
* [Rationale](#rationale)
* [Drawbacks](#drawbacks)
* [Prior Art](#prior-art)
* [Alternatives](#alternatives)
  * [For transport between Agents and Vector](#for-transport-between-agents-and-vector)
  * [Flattening sketches](#flattening-sketches)
  * [For Request routing](#for-request-routing)
* [Outstanding Questions](#outstanding-questions)
* [Plan Of Attack](#plan-of-attack)
* [Future Improvements](#future-improvements)

## Context

* Vector is foreseen as a Datadog Agents aggregator, thus receiving metrics from Datadog Agents is a logical development
* Vector has support to send metrics to Datadog, thus receiving metrics from Agent is a consistent feature to add

## Cross cutting concerns

Some known issues are connected to the work described here: [#7283], [#8493] & [#8626]. This mostly concerns the ability
to store/manipulate distribution using [sketches], send those to Datadog using the DDSketch representation. Other
metrics sinks would possibly benefit from having distribution stored internally with sketches as this would provide
better aggregation and accuracy.

[#7283]: https://github.com/vectordotdev/vector/issues/7283
[#8493]: https://github.com/vectordotdev/vector/issues/8493
[#8626]: https://github.com/vectordotdev/vector/issues/8626
[sketches]: http://www.vldb.org/pvldb/vol12/p2195-masson.pdf

## Scope

### In scope

* Implement a Datadog metrics endpoint in Vector, it will match the [metrics intake
  API](https://docs.datadoghq.com/api/latest/metrics/) with additional route that the Agent uses
* Include support for sketches that uses protobuf.
* Ensure all Datadog metrics type are mapped to internal Vector metric type and that there is no loss of accuracy in a
  pass through configuration.

### Out of scope

* Anything not related to metrics
  * Processing API validation requests
  * Processing other kind of payloads: traces, event, etc.
* Shipping sketches to Datadog in the `datadog_metrics` sinks, it is required reach a fully functional situation but
  this is not the goal of this RFC that focus on receiving metrics from Datadog Agents.

## Pain

* Users cannot aggregate metrics from Datadog agents

## Proposal

### User Experience

* Vector will support receiving Datadog Metrics sent by the official Datadog Agent through a standard source
* Metrics received will be fully supported inside Vector, all metric types will be supported
* The following metrics flow: `n*(Datadog Agents) -> Vector -> Datadog` should just work
* No foreseen backward compatibility issue (tags management may be a bit bothersome)
* New configuration settings should be consistent with existing ones

Regarding the Datadog Agent configuration, ideally it should be only a matter of configuring `metrics_dd_url:
https://vector.mycompany.tld` to forward metrics to a Vector deployment.

The current `dd_url` endpoint configuration has a [conditional
behavior](https://github.com/DataDog/datadog-agent/blob/main/pkg/config/config.go#L1199-L1201) (also
[here](https://github.com/DataDog/datadog-agent/blob/main/pkg/forwarder/forwarder_health.go#L131-L143)). I.e. if
`dd_url` contains a known pattern (i.e. it has a suffix that matches a Datadog site) some extra hostname manipulation
happens. But overall, the following paths are expected to be supported on the host behind `dd_url`:

* `/api/v1/validate` for API key validation
* `/api/v1/check_run` for check submission
* `/intake/` for events and metadata (possibly others)
* `/support/flare/` for support flare
* `/api/v1/series` & `/api/beta/sketches` for metrics submission

Then to only ship metrics, and let other payload follow the standard path, the newly introduced Datadog Agent setting
`metrics_dd_url` would have to be set to point to a Vector host, with a `datadog_agent` source enabled. And then request
targeted to `/api/v1/series` & `/api/beta/sketches` would be diverted there allowing Vector to further processed them.

### Implementation

A few details about the Datadog Agents & [Datadog metrics](https://docs.datadoghq.com/metrics/types/):

* The base structure for all metrics is named
  [`MetricSample`](https://github.com/DataDog/datadog-agent/blob/main/pkg/metrics/metric_sample.go#L81-L94) and can be
  of [several types](https://github.com/DataDog/datadog-agent/blob/main/pkg/metrics/metric_sample.go#L20-L31)
* Major Agent usecases:
  * Metrics are send from corechecks (i.e. go code)
    [here](https://github.com/DataDog/datadog-agent/blob/main/pkg/aggregator/sender.go#L227-L252)
  * Dogstatsd metrics are converted to the `MetricSample` structure
    [here](https://github.com/DataDog/datadog-agent/blob/main/pkg/dogstatsd/enrich.go#L87-L137) However Datadog Agents
    metrics are transformed before being sent, ultimately metrics accounts for two different kind of payload:
* The count, gauge and rate series kind of payload, sent to `/api/v1/series` using the [JSON schema officially
  documented](https://docs.datadoghq.com/api/latest/metrics) with few undocumented [additional
  fields](https://github.com/DataDog/datadog-agent/blob/main/pkg/metrics/series.go#L45-L57), but this align very well
  with the existing `datadog_metrics` sinks.
* The sketches kind of payload, sent to `/api/beta/sketches` and serialized as protobuf as shown
  [here](https://github.com/DataDog/datadog-agent/blob/main/pkg/serializer/serializer.go#L315-L338) (it ultimately lands
  [here](https://github.com/DataDog/datadog-agent/blob/main/pkg/metrics/sketch_series.go#L103-L269)). Public `.proto`
  definition can be found
  [here](https://github.com/DataDog/agent-payload/blob/master/proto/metrics/agent_payload.proto#L47-L81).

Vector has a nice description of its [metrics data
model](https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/) and a [concise enum for
representing it](https://github.com/vectordotdev/vector/blob/master/lib/vector-core/src/event/metric.rs#L135-L169).


The implementation would then consist in:

* Implement a Datadog Agent change and introduce a new override (let's say `metrics_dd_url`) that would only divert
   request to `/api/v1/series` & `/api/beta/sketches` to a specific endpoints.
* Handle the `/api/v1/series` route (based on both the official [API](https://docs.datadoghq.com/api/latest/metrics/) and
  the [Datadog Agent itself](https://github.com/DataDog/datadog-agent/blob/main/pkg/forwarder/telemetry.go#L20-L31)) to
  cover every metric type handled by this endpoint (count, gauge and rate) and:
  * Add support for missing fields in the `datadog_metrics` sinks
  * The same value but different keys tags (Datadog allows `key:foo` & `key:bar` but Vector doesn't) maybe supported
      later if there is demand for it (see the note below).
  * Overall this is fairly straightforward
* Handle the `/api/beta/sketches` route in the `datadog_agent` source to support sketches/distribution encoded using
  protobuf, but once decoded those sketches will require internal support in Vector:
  * Distribution metrics in the `datadog_metrics` sink would need to use sketches and the associated endpoint. This is
     a prerequisite to support end-to-end sketches forwarding.
  * The sketches the agent ships is based on this [paper](http://www.vldb.org/pvldb/vol12/p2195-masson.pdf) whereas
     Vector uses what's called a summary inside the Agent, implementing the complete DDSketch support in Vector is
     probably a good idea as sketches have convenient properties for wide consistent aggregation and limited error. To
     support smooth migration, full DDsktech (or compatible sketch) support is mandatory, as customers that emit
     distribution metric from Datadog Agent would need it to migrate to Vector aggregation. This RFC assumes there will
     be a complete sketch metric (likely to be DDSketch) that would be compatible and support the following scenario
     without loss of information: `(Agent Sketch) -> (Vector) -> (Datadog intake)`. This RFC focus on ingesting sketch
     and not the rest of the flow.

**Regarding the tagging issue:** A -possibly temporary- workaround would be to store incoming tags with the complete
"key:value" string as the key and an empty value to store those in the existing map Vector uses to store
[tags](https://github.com/vectordotdev/vector/blob/master/lib/vector-core/src/event/metric.rs#L60) and slightly rework the
`datadog_metrics` sink not to append `:` if a tag key has the empty string as the corresponding value. However Datadog
best practices can be followed with the current Vector data model, so unless something unforeseen or unexpected demand
arise, Vector internal tag representation will not be changed following this RFC.


## Rationale

* Smoother Vector integration with Datadog.
* Needed for Vector to act as a complete Datadog Agent aggregator (but further work will still be required).
* Extend the Vector ecosystem, bring additional feature for distribution metrics that would enable consistent
  aggregation.

## Drawbacks

Users that would want to use this feature will need to upgrade both Vector and the Agent. If a new metric route comes up
in a Datadog Agent upgrade, users will need to upgrade Vector as well.

## Prior Art

There are few existing metric aggregation solution. The Datadog Agent is able to
[aggregate](https://github.com/DataDog/datadog-agent/tree/main/pkg/aggregator), in some extend, metrics coming over
dogstatsd and from go/python code. It mostly aims at reducing the amount of metrics samples sent by the Agent.

[Veneur](https://github.com/stripe/veneur#global-aggregation) offers an aggregation feature, but it does not support
sketches/distribution per se. It requires what is called a central veneur, that would compute aggregated value for
selected metrics and some percentile. Some aspects of this solution could be seen as an
[alternative](#flattening-sketches) approach. However this approach has two major drawbacks: it relies on a central
service for aggregation and it does not support sketches.

## Alternatives

### For transport between Agents and Vector

The use an alternate protocol between Datadog Agents and Vector (Like Prometheus, Statds, OpenTelemetry or Vector own
protocol) could be envisioned. This would call for a significant, yet possible with the current Agent architecture,
addition, those changes would mostly be located in the
[forwarder](https://github.com/DataDog/datadog-agent/tree/main/pkg/forwarder) and
[serializer](https://github.com/DataDog/datadog-agent/tree/main/pkg/serializer) logic. This would imply a hugh chunk of
work on the Agent side, require update to use the feature, probably also require some work on Vector side. This is not
something that aligns well with the purpose of the Datadog Agent. This would also add a risk of losing information
because of protocol conversion.

### Flattening sketches

For sketches, we could flatten sketches and compute usual derived metrics (min/max/average/count/some percentiles) and
send those as gauge/count, but it would prevent (or at least impact) existing distribution/sketches users. Moreover if
instead of sketches only derived metrics are used a lot of the tagging flexibility will be lost. By submitting tagged
sketches to the Datadog intake, any tag selector can be used to compute a distribution based on the sketches that bear
matching tag. This cannot be done without sending sketches. But flattening sketches would have the benefit of simplify
the implementation in Vector and remove the prerequisite of having sketches support inside Vector.

### For Request routing

Instead of being done in the Agent, the request routing could be implemented either:

1. In Vector, that would receive both metric and non-metric payload, simply proxying non-metric payload directly to
   Datadog without further processing.
2. Or in a third party middle layer (e.g. haproxy or similar). It could leverage the [documented
   haproxy](https://docs.datadoghq.com/agent/proxy/?tab=agentv6v7#haproxy) setup for Agent to divert selected routes to
   Vector, but it would have the advantage of resolving any migrations, not-yet-supported-metric-route in Vector and
   alleviate the need of modifying the Agent.

**Note**: proxying non-metric request is not a completely discarded option, as this might still be useful in some
situation where proxying everything is explicitly wanted or where proxying unknown payload (for example if the Agent is
upgraded and comes with a new metric route not yet supported by Vector) would serve as a data loss prevention mechanism
and/or help to maintain metric continuity.

## Outstanding Questions

None

## Plan Of Attack

* [ ] Implement a new `metrics_dd_url` overrides in the Datadog Agent
* [ ] Support `/api/v1/series` route still in the `datadog_agent` source, implement complete support in the
  `datadog_metrics` sinks for the undocumented fields, incoming tags would be stored as key only with an empty string
  for their value inside Vector. Validate the `Agent->Vector->Datadog` scenario for gauge, count & rate.
* [ ] Support `/api/beta/sketches` route, again in the `datadog_agent`, and validate the `Agent->Vector->Datadog`
  scenario for sketches/distributions. This would also required internal sketches support in Vector along with sending
  sketches from the `datadog_metrics` sinks, this is not directly addressed by this RFC but it is tracked in the
  following issues: [#7283](https://github.com/vectordotdev/vector/issues/7283),
  [#8493](https://github.com/vectordotdev/vector/issues/8493) & [#8626](https://github.com/vectordotdev/vector/issues/8626).

The later task depends on the issue [#9181](https://github.com/vectordotdev/vector/issues/9181).

## Future Improvements

* Wider use of sketches for distribution aggregation.
* Expose some sketches function in VRL (at least merging sketches).
* Continue on processing other kind of Datadog payloads.
