# RFC 9460 - 2021-12-01 - Healthcheck endpoint improvements

Our existing health endpoint is too limited and additional features are required
to properly interact with load balancers or Kubernetes service discovery.

## Context

- [Additional response for api health endpoint](https://github.com/vectordotdev/vector/issues/9160)

## Scope

### In scope

- Update health endpoint to improve Vector's reliability in production deployments
- Update default configurations to improve Vector's out-of-the-box reliability

### Out of scope

- Moving the health endpoint out of our existing API
- Adding user facing configuration to adjust endpoint behavior or responses

## Pain

- Today's health endpoint causes 502's when running behind a loadbalancer
- The healthcheck isn't integrated with any components and only represents if Vector
itself is running

## Proposal

### User Experience

The existing health endpoint on Vector's API is enhanced to return 503's
when Vector is shutting down. This additional response will allow load balancers
and service discovery to better integrate with Vector, sources already begin
rejecting requests received during the shutdown process and updating the health
check will allow for removing the instance from available backends and avoid
routing traffic to an instance that will end up rejecting the request regardless.

### Implementation

We have existing logic in place for other components to handle Vector's shutdown.
The [health handler](https://github.com/vectordotdev/vector/blob/master/src/api/handler.rs#L7)
can be updated to respond 503 if shutdown has started and 200 otherwise. This
should be a minor change and not negatively impact existing deployments.

## Rationale

This improvement is low effort but greatly improves the experience of running
Vector behind a load balancer or in a service discovery system. As we recommend
both of those options for production deployments there's very little reason not
to implement this.

## Drawbacks

This is a minor change overall and does not add any real engineering burden.
It does open the door for further health check improvements which are more
complex. Any further improvements to the check need to be planned and discussed
to avoid unwanted complexity.

## Prior Art

- [Beats](https://www.elastic.co/guide/en/beats/filebeat/7.15/http-endpoint.html)
  - [Liveness and Readiness Probes](https://github.com/elastic/helm-charts/blob/715eeda8a45b8c3d8542921f5485aa502c238d93/filebeat/values.yaml#L174-L198)
- [FluentBit](https://docs.fluentbit.io/manual/administration/monitoring#rest-api-interface)
  - [Liveness and Readiness Probes](https://github.com/fluent/helm-charts/blob/355575c5b2a5bd858bcadeaa9d8d5d7f15a7816d/charts/fluent-bit/values.yaml#L132-L140)

## Alternatives

### Do nothing

Not adding additional responses to the existing health endpoint seems strictly
worse, as we don't have any ways to stop routing traffic to instances that are
shutting down. Those requests will already be rejected by sources and this
behaviour reflects poorly on zero downtime deployments, even though events
shouldn't be dropped.

### Aggregate per component health

We could instead look to define "healthiness" per component and aggregate/blend
these for the global health endpoint. Looking at a single Vector instance from
a high level this is similar to calculating a simple up/down metric for a
distributed system. We would need to determine how many errors/failures per
component cause them to be "unhealthy", and depending on the component and config
this could be very situational. This would require user provided configuration
and increase operational complexity, something we intentionally avoid when possible.

## Outstanding Questions

- ~~Should we assign a unique error code for "shutting down"?~~
- Do we want to return "unhealthy" if some number of components are "unhealthy"
  - Depending on deployment and configuration this could push durability concerns
to an agent layer, which wouldn't necessarily be ideal
  - This could be introduced down the line as a configurable option
- Related to the previous, should sinks (or other components) regularly rerun
their configured healthchecks or should it continue to be at startup only
- Using `vector validate` as a Kubernetes Readiness Probe could take an instance
out of service discovery and push buffering and durability to a less optimal layer
  - This is probably a better startup check but may not be ideal for a regular
runtime check, if that's the case we should use a Startup Probe

## Plan Of Attack

- [ ] Integrate health endpoint with our shutdown sequence, letting the API return
an unhealthy code and take the shutting down instance out of load balancing/service
discovery

## Future Improvements

- Add routes/optional params to health endpoint to query the health of specific
components
- Add a "tiered" health status (Green/Yellow/Red) to better represent the "distributed"
nature of Vector's runtime (Elasticsearch health endpoints as an example)
- Add a built-in command similar to `validate` intended to check instance health at runtime.
- Add healthchecks for sources as well as sinks, to better determine Vector's ability
to receive events
