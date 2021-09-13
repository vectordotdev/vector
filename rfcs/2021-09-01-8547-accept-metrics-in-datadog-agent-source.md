# RFC 8547 - 2021-09-01 - Accept metrics in `datadog_agent` source

Currently the `datadog_agent` [source](https://vector.dev/docs/reference/configuration/sources/datadog_agent/) only
supports logs. This RFC suggests to extend Vector to support receiving metrics from Datadog agents and ingest those as
metrics from a Vector perspective so they can be benefit from Vector capabilities.

  * [Context](#context)
  * [Cross cutting concerns](#cross-cutting-concerns)
  * [Scope](#scope)
    + [In scope](#in-scope)
    + [Out of scope](#out-of-scope)
  * [Pain](#pain)
  * [Proposal](#proposal)
    + [User Experience](#user-experience)
    + [Implementation](#implementation)
  * [Rationale](#rationale)
  * [Drawbacks](#drawbacks)
  * [Prior Art](#prior-art)
  * [Alternatives](#alternatives)
  * [Outstanding Questions](#outstanding-questions)
  * [Plan Of Attack](#plan-of-attack)
  * [Future Improvements](#future-improvements)

## Context
- Vector is foreseen as a Datadog Agents aggregator, thus receiving metrics from Datadog Agents is a logical development
- Vector has support to send metrics to Datadog, thus receiving metrics from Agent is a consistent feature to add

## Cross cutting concerns

Some known issues are connected to the work described here: [#7283](https://github.com/timberio/vector/issues/7283),
[#8493](https://github.com/timberio/vector/issues/8493) & [#8626](https://github.com/timberio/vector/issues/8626). This
mostly concerns the ability to store/manipulate distribution using sketches, send those to Datadog using the DDSketch
representation. Other metrics sinks would possibly benefit from having distribution stored internally with sketches as
this would provide better aggregation and accuracy.

## Scope

### In scope

- Implement a Datadog metrics endpoint in Vector, it will match the [metrics intake
  API](https://docs.datadoghq.com/api/latest/metrics/) with additional route that the Agent uses
- Include support for sketches that uses protobuf.
- Ensure all Datadog metrics type are mapped to internal Vector metric type and that there is no loss of accuracy in a
  pass through configuration.

### Out of scope

- Anything not related to metrics
  - Processing API validation requests
  - Processing other kind of payloads: traces, event, etc.
- Shipping sketches to Datadog in the `datadog_metrics` sinks, it is required reach a fully functionnal situation but
  this is not the goal of this RFC that focus on receiving metrics from Datadog Agents.

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

Regarding the Datadog Agent configuration, ideally it should be only a matter of configuring `dd_url:
https://vector.mycompany.tld` to forward metrics to a Vector deployement. However configuring this will lead to some
non-metric payloads to be sent to Vector, while it seems not an option to change the Datadog Agent behavior, those
non-metric payload should still be forwarded to Datadog.

The `dd_url` endpoint configuration has a [conditional
behavior](https://github.com/DataDog/datadog-agent/blob/main/pkg/config/config.go#L1199-L1201) (also
[here](https://github.com/DataDog/datadog-agent/blob/main/pkg/forwarder/forwarder_health.go#L131-L143)). I.e. if
`dd_url` contains a known pattern (i.e. it has a suffix that matches a Datadog site) some extra hostname manipulation
happens. But overal, this conditional can be ignore here and if we want unmodified Datadog Agent to use a `dd_url`
pointing to a Vector deployement, the following route will have to be supported:
- `/api/v1/validate` for API key validation
- `/api/v1/check_run` for check submission
- `/intake/` for events and metadata (possibly others)
- `/support/flare/` for support flare
- `/api/v1/series` & `/api/beta/sketches` for metrics submission

Regarding the relative amount of requests:
- `/api/v1/validate` is requested at a periodic interval (10 seconds).
- `/intake/` is requested at various (rather long)
  [intervals](https://github.com/DataDog/datadog-agent/blob/main/pkg/metadata/helper.go#L12-L22).
- `/support/flare/` is only used by users upon customer support request, but payloads can be rather large (>10MBytes).
- `/api/v1/series` & `/api/beta/sketches` it accounts for the vast majority of requests, it relays everything received
  on the Dogstatsd socket by the Agent.
- `/api/v1/check_run` depends on the number of checks, but it usually accounts for much less data than metrics.

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

- Implement in the `datadog_agent` source the ability to proxy all known route (see above) as-is directly to Datadog
  (this will require an new settings in the Vector source, something like `unused_payload_site: us3.datadoghq.com`). At
  the beginning it will also proxify `/api/v1/series` & `/api/beta/sketches`.
- Handle the `/api/v1/series` route (based on both the offical [API](https://docs.datadoghq.com/api/latest/metrics/) and
  the [Datadog Agent itself](https://github.com/DataDog/datadog-agent/blob/main/pkg/forwarder/telemetry.go#L20-L31)) to
  cover every metric type handled by this endpoint (count, gauge and rate) and:
    - Add support for missing fields in the `datadog_metrics` sinks
    - The same value but different keys tags (Datadog allows `key:foo` & `key:bar` but Vector doesn't) maybe supported later if there is demand for it (see the note below).
    - Overall this is fairly straighforward
- Handle the `/api/beta/sketches` route in the `datadog_agent` source to support sketches/distribution encoded using
  protobuf
   - Revamp the distribution metrics in the `datadog_metrics` sink to use sketches and the associated endpoint will
     probably be a good idea.
   - The sketches the agent ships is based on this [paper](http://www.vldb.org/pvldb/vol12/p2195-masson.pdf) whereas
     Vector uses what's called a summary inside the Agent, implementing the complete DDSketch support in Vector is
     probably a good idea as sketches have convenient properties for wide consistent aggregation and limited error. To
     support smooth migration, full DDsktech support is mandatory, as customers that emit distribution metric from
     Datadog Agent would need it to migrate to Vector aggegation.

**Regarding the tagging issue:** A -possibly temporary- work-around would be to store incoming tags with the complete
"key:value" string as the key and an empty value to store those in the extisting map Vector uses to store
[tags](https://github.com/timberio/vector/blob/master/lib/vector-core/src/event/metric.rs#L60) and slightly rework the
`datadog_metrics` sink not to append `:` if a tag key has the empty string as the corresponding value. However Datadog best
practices can be followed with the current Vector data model, so unless something unforeseen or unexpected demand arise,
Vector internal tag represention will not be changed following this RFC.


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
- Opentelemetry is moving to official support sketches on a [general
  basis](https://github.com/open-telemetry/opentelemetry-specification/issues/982).

## Alternatives

- Use an alternate protocol between Datadog Agents and Vector (Like Prometheus, Statds or Vector own protocol)
- For sketches, we could flatten sketches and compute usual derived metrics (min/max/average/count/some percentile) and
  send those as gauge/count, but it would prevent (or at least impact) existing distribution/sketches user
- Regarding how we support route and the fact that Vector would have to support non-metric payload, an alternate
  solution would be to push for a Datadog Agent change with a new override (let's say `dd_metrics_url` that would only
  divert request to `/api/v1/series` & `/api/beta/sketches` to a specific endpoints. This however highlights that Vector
  & the Datadog Agent will then need to follow a compatbility matrix:
  1. If the Agent is upgrade with a new metric route (let's say for v2 intake migration), the user will need to also
     upgrade Vector so it supports this route.
  2. Existing fleet of Agents will need to be upgraded to be able to send metrics to Vector.

  While #2 is not really a major issue as this would be a one shot upgrade if required, #1 may be more bothersome as the
  issue will always be there. A somehow hacky solution would be to leverage the [documented
  haproxy](https://docs.datadoghq.com/agent/proxy/?tab=agentv6v7#haproxy) setup for Agent to divert selected routes to
  Vector, but it would have the advantage of resolving any migrations, not-yet-supported-metric-route in Vector and
  alleviate the need of modifying the Agent.

## Outstanding Questions

- Datadog Agent configuration, diverting metrics to a custom endpoints will likely divert other kind of traffic, this
  need to be clearly identified / discussed
- Is there any other metrics type that could required some degree of adaptation, there is the non monotonic counter that
  exists in the agent but not in Vector that may require the introduction of a so-called `NonMonotonicCounter` in
  Vector.

## Plan Of Attack

- [ ] Implement the proxy logic in the `datadog_agent` source
- [ ] Support `/api/v1/series` route still in the `datadog_agent` source, implement complete support in the
  `datadog_metrics` sinks for the undocumented fields, incoming tags would be stored as key only with an empty string
  for their value inside Vector. Validate the `Agent->Vector->Datadog` scenario for gauge, count & rate.
- [ ] Extend Vector to support sketches following the DDSketch paper, implement sketches forwarding in the
  `datadog_metrics` sinks.
- [ ] Support `/api/beta/sketches` route, again in the `datadog_agent`, and validate the `Agent->Vector->Datadog`
  scenario for sketches/distributions. This would also required sending sketches from the `datadog_metrics` sinks, this
  is not directly addressed by this RFC but it is tracked in the following issues:
  [#7283](https://github.com/timberio/vector/issues/7283), [#8493](https://github.com/timberio/vector/issues/8493) &
  [#8626](https://github.com/timberio/vector/issues/8626).

## Future Improvements

- Wider use of sketches for distribution aggregation.
- Expose some sketches function in VRL (at least merging sketches).
- Continue on processing other kind of Datadog payloads.
