# RFC 9460 - 2021-12-01 - Health endpoint improvements

Our existing `/health` endpoint is limited and additional information should be
exposed for both operators and load balancers/service discovery.

## Context

- [Additional response for api health endpoint](https://github.com/vectordotdev/vector/issues/9160)
- [Formalize component health to include statuses and runtime health](https://github.com/vectordotdev/vector/issues/10555)
- [Add specification for the `/health` endpoint](https://github.com/vectordotdev/vector/issues/10556)
- [Add component health status the `/heath` endpoint](https://github.com/vectordotdev/vector/issues/9469)

## Scope

### In scope

- Update health endpoint to improve Vector's reliability in production deployments
- Update default configurations to improve Vector's out-of-the-box reliability
- Improve visibility of high level status at runtime

### Out of scope

- Immediate integrations to expose component level health

## Pain

- Today's health endpoint causes 502's when running behind a loadbalancer
- The healthcheck isn't integrated with any components and only represents if Vector
itself is running
- Operators don't have visibility into the state of specific components, ex. is
backpressure being applied to upstream components

## Proposal

### User Experience

The existing health endpoint on Vector's API is updated to return 503's
when Vector is shutting down. This additional response will allow load balancers
and service discovery to better integrate with Vector, sources already begin
rejecting requests received during the shutdown process and updating the health
check will allow for removing the instance from available backends and avoid
routing traffic to an instance that will end up rejecting the request regardless.

The API will not start until the topology is build and validated as functional,
this isn't precisely when Vector's configured sources _can_ actually process
events but it's a reasonable first step to ensure graceful startups.

### Implementation

We have existing logic in place for other components to handle Vector's shutdown.
The [health handler](https://github.com/vectordotdev/vector/blob/master/src/api/handler.rs#L7)
can be updated to respond 503 if shutdown has started and 200 otherwise. This
should not negatively impact existing deployments.

## Rationale

This improvement is low effort but greatly improves the experience of running
Vector behind a load balancer or in a service discovery system. As we recommend
both of those options for production deployments there's very little reason not
to implement this.

## Drawbacks

N/A

## Prior Art

- [Beats](https://www.elastic.co/guide/en/beats/filebeat/7.15/http-endpoint.html)
  - [Liveness and Readiness Probes](https://github.com/elastic/helm-charts/blob/715eeda8a45b8c3d8542921f5485aa502c238d93/filebeat/values.yaml#L174-L198)
- [FluentBit](https://docs.fluentbit.io/manual/administration/monitoring#rest-api-interface)
  - [Liveness and Readiness Probes](https://github.com/fluent/helm-charts/blob/355575c5b2a5bd858bcadeaa9d8d5d7f15a7816d/charts/fluent-bit/values.yaml#L132-L140)
- [Elasticsearch](https://www.elastic.co/guide/en/elasticsearch/reference/7.16/cluster-health.html)
  - [Readiness Probe](https://github.com/elastic/helm-charts/blob/715eeda8a45b8c3d8542921f5485aa502c238d93/elasticsearch/templates/statefulset.yaml#L227-L291)
- [Datadog Agent]
  - [Liveness and Readiness Probes](https://github.com/DataDog/helm-charts/blob/d5e1f4370442bdc5e457468ac7ff0ff943f528d5/charts/datadog/templates/_container-agent.yaml#L193-L199)
  - [agent health](https://docs.datadoghq.com/agent/guide/agent-commands/?tab=agentv6v7#other-commands)

## Alternatives

### Do nothing

Not adding additional responses to the existing health endpoint seems strictly
worse, as we don't have any ways to stop routing traffic to instances that are
shutting down. Those requests will already be rejected by sources and this
behaviour reflects poorly on zero downtime deployments, even though events
shouldn't be dropped.

### `vector health` subcommand

While not suitable for general load balancer usage, it's very easy to `exec`
commands in Kubernetes to determine health. For prior art we don't have to look
further than the Datadog Agent which has a subcommand that outputs current
health information that can be used by operators/systems.

## Outstanding Questions

- Do we need to update the `{ health }` GraphQL query at the same time?
- ~~Should we assign a unique error code for "shutting down"?~~

## Plan Of Attack

- [ ] Integrate health endpoint with our shutdown sequence, having the API return
a `503` and take the shutting down instance out of load balancing/service discovery
- [ ] Verify the `/health` endpoint is unavailable until Vector's topology is build
and valid to run

## Future Improvements

- Expand Vector's concept of "health" to a component level, and define a spec for
both the `/health` endpoint as well as component "health"
- Add routes/optional params to health endpoint to query the health of specific
components
- Add a "tiered" health status (Green/Yellow/Red) to better represent the "distributed"
nature of Vector's runtime (Elasticsearch health endpoints as an example)
- Add healthchecks for `sources` (and `transforms`), to better determine Vector's
ability to receive and process events
- Update sinks (and other components) regularly rerun their configured healthchecks
