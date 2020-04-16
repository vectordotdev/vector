# RFC 1858 - 2020-04-06 - Automatically Adjust Limits

This RFC proposes a new scheme for rate limiting requests to external
services in order to maximize the sustained transmission rate over
varying conditions.

## Motivation

Vector users commonly run into the problem of internal service rate
limiting. This is not an external service limiting receiving data from
us but our own rate limiting based on the `request`
parameters. Alternatively, users can run into the opposite problem of
overwhelming a downstream service to the point where it becomes
unresponsive and starts queueing requests. Instead, Vector should be
automatically rate limiting its requests to maximally fill the service's
capacity without overwhelming it and causing additional problems.

Most sinks in Vector have their request structure managed by the [tower
crate](https://github.com/tower-rs/tower).  This service builder allows
for setting how requests are sent to remote sinks. In particular, Vector
fixes the number of requests that may be simultaneously in flight (AKA
the concurrency limit), the maximum rate at which requests may be sent,
expressed in terms of the number of requests over some time interval
(AKA the rate limit number and duration).

Many of these parameters _must_ be adjusted by the Vector administrator
to maximize the throughput for each service. For high volume sites, this
can require considerable trial and error experimentation before a
satisfactory set of parameters can be achieved. More importantly,
changes in the service parameters, whether in terms of available
processing power, bandwidth, or changes to latency, such as is caused by
link congestion, or the number of other agents delivering to the same
sink, can cause the same settings to now impede utilization rather than
improve it.

Since many of these factors that affect delivery rate are not fixed
values but instead will vary considerably throughout the life of a
service, it is impossible to choose the "best" parameters that will fit
all these conditions. Instead, Vector should adopt an approach that
allows for dynamic adjustment of the underlying parameters based on the
current conditions.

When service limits are reached, Vector will experience a number of
undesirable phenomenon, notably request latency, timeouts, and
deferrals. These all decrease the overall flow rate while actually
increasing actual bandwidth usage.

## Guide-level Proposal

There are two levels of controls we have in play—static and dynamic. The
existing controls are static, and best describe service limits, such as
maximum allowed request rates. What is needed is a set of dynamic
controls that adapt to the underlying conditions and scale the service
utilization on the fly.

Since the controls under consideration are dependant on some form of
queueing, the controls will be inserted at the same level as
`TowerRequestSettings`. The existing rate limit controls will remain in
order to provide a hard upper bound on service utilization (for example,
to prevent over-use violations), but will be dynamically bounded by
adjusting the concurrency.

The underlying control will replace the `tower::limit::ConcurrencyLimit`
layer with a new custom layer that dynamically adjusts the concurrency
limit based on current conditions. It will track each request's result
status (ie success, or deferral) and the round trip time (RTT). This
will require a modified `tower::limit::Limit` structure that will add
and remove permits as needed. A new `ResponseFuture` will forward the
result of the request back to the invoking `ConcurrencyLimit` after a
completed `poll` (in addition to the usual action of releasing the
permit on `drop`).

The algorithm used to control the limit will follow the AIMD framework:

* The controller will maintain an moving average RTT of past requests
  using an exponentially weighted moving average (EWMA). The weighting
  (α) is to be experimentally determined.

* The current response's RTT is compared to this moving average:

  * If less than or equal to the average, the concurrency will be
    incremented by one (additive increase) up to a maximum of the in
    flight limit.

  * If greater than the average, or the result was a failure of any
    kind, the concurrency will be reduced by a factor of one half
    (multiplicative decrease) down to a minimum of one.

## Prior Art

* [TCP congestion control algorithms](https://en.wikipedia.org/wiki/TCP_congestion_control)
* [Additive Increase/Multiplicative Decrease](https://en.wikipedia.org/wiki/Additive_increase/multiplicative_decrease)
* [Netflix Technology Blog: Performance Under Load](https://medium.com/@NetflixTechBlog/performance-under-load-3e6fa9a60581)
* [JINSPIRED - Adaptive Safety Control (archive.org)](https://web.archive.org/web/20130105023839/http://www.jinspired.com/site/jxinsight-opencore-6-4-ea-11-released-adaptive-safety-control)

## Sales Pitch

This proposal:

* provides a simple and understandable mechanism for varying resource
  utilization of sinks;

* adapts an existing design to avoid reinventing known good solutions;

* is minimally invasive to the existing code base while applying to most
  sinks;

* minimizes the amount of configuration required to produce the ideal
  (most efficient and performant) configuration; and

* does not impose hard limits on flow rates while respecting configured
  limits.

## Drawbacks

Since the underlying parameters that control when requests are throttled
will be abstracted behind an additional layer, it will become harder to
reason about the causes of bandwidth limits.

## Rationale

* As referenced earlier, the proposed mechanism borrows from _proven_
  mechanisms designed to manage flow control under varying conditions,
  making it a good choice for the first pass implementation.

* A moving average is used to smooth out small variations in latency
  without completely ignoring them.

* EWMA is chosen as an averaging mechanism as it avoids having to
  maintain memory of past observations beyond a single
  value. Mathematically it is the simplest possible moving average.

## Alternatives

* Instead of managing the concurrency, we could alter the maximum
  request rate or maximum bandwidth usage. This runs into the difficulty
  of how to set the minimum bound both before any data has been
  collected and after hard backoffs, while concurrency has a trivially
  obvious minimum bound and is better able to flex with load.

* Instead of comparing the RTT against a moving average, we could simply
  use the previous observation (mathematically equivalent to a EWMA
  weighting of α=1).

## Outstanding Questions

* The ideal value for the weighting α is unknown. Too large a value will
  amplify the effect of short term changes to RTT. Too small a value may
  delay responding to real changes excessively.

* Some experimentation may be required to determine a small zone around
  the average that is still considered "equal" to avoid excessive
  flapping of the concurrency level without allowing the RTT to grow
  unbounded and overload the sink.

* Some level of (random) jitter may be needed to stagger the increases,
  to avoid a large number of clients overwhelming a sink.

## Plan Of Attack

* [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
* [ ] Benchmark the approach under various conditions to determine a good
      value for α.
* [ ] ………
