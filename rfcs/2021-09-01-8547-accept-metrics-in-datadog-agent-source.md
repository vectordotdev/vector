# RFC 8547 - 2021-09-01 - Accept metrics in `datadog_agent` source

Currently the `datadog_agent` [source](https://vector.dev/docs/reference/configuration/sources/datadog_agent/) only
supports logs. This RFC suggests to extend Vector to support receiving metrics from Datadog agents and ingest those as
metrics from a Vector perspective so they can be benefit from Vector capabilities.

## Context
- Vector is foreseen as a Datadog Agents aggregator, thus receiving metrics from Datadog Agents is a logical development
- Vector has support to send metrics to Datadog, thus receiving metrics from Agent is a consistent feature

## Cross cutting concerns

N/A

## Scope

### In scope

- Implement a Datadog metrics endpoint in Vector, it will match the [metrics intake
  API](https://docs.datadoghq.com/api/latest/metrics/) with additional route that the Agent uses.
- Ensure all Datadog metrics type are mapped to internal Vector metric type and that there is no loss
- TBC if needed: internal Vector adjustement, probably required for sketches

### Out of scope

- Anything not related to metrics
  - API validation requests
  - Other kind of payloads: traces, event, etc.)

## Pain

- Vector cannot fully aggregate Datadog Agent traffic
- Inconsistency between Datadog logs vs. metrics support in Vector

## Proposal

### User Experience

- Vector will support receiving Datadog Metrics sent by the official Datadog Agent through a standard source
- Metrics received will be fully supported inside Vector, all metric types will be supported
- The following metrics flow: `n*(Datadog Agents) -> Vector -> Datadog` should just work
- No foreseen backward compatibily issue (tags management may bring some )
- New configuration settings should be consistent with existing ones

### Implementation

- Add route for metrics in the existing `datadog_agent` source (based on both the offical
  [API](https://docs.datadoghq.com/api/latest/metrics/) and the [Datadog Agent
  itself](https://github.com/DataDog/datadog-agent/blob/main/pkg/forwarder/telemetry.go#L20-L31)) to cover every metric
  that can be sent by the Agent.
- TBC: tags consitentcy (list in Datadog vs map in Vector)
- TBC: full metrics type coverage

## Rationale

- Smoother integration with Datadog
- Needed for Vector to act as a complete Datadog Agent aggregator

## Drawbacks

- TBD

## Prior Art

- Existing Vector metrics source over HTTP(s) works well
- Datadog logs is already supported in the `datadog_agent` source
- Vector has its own vector-to-vector protocol that serve a similar purpose

## Alternatives

- Use an alternate protocol between Datadog Agents and Vector

## Outstanding Questions

- Datadog Agent configuration, diverting metrics to a custom endpoints will likely divert other kind of traffic, this
  need to be clearly identified / discussed

## Plan Of Attack

- [ ] Support the publicly documented route to the `datadog_agent` source for metrics type compatible with Vector
- [ ] Extend Vector to support sketches and and the sketches route
- [ ] Support payloads (must be clearly identified) that are sent with metrics by the Agent

## Future Improvements

TBD