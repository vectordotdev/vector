# RFC 1858 - 2020-04-06 - Automatically Adjust Request Limits

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
the concurrency limit or `in_flight_limit`) and the maximum rate at
which requests may be sent, expressed in terms of the number of requests
over some time interval (AKA the rate limit number and duration).

Many of these parameters _must_ be adjusted by the Vector operator to
maximize the throughput for each service. For high volume sites, this
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

### Control Mechanism

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
adjusting the concurrency. An additional control will be used to
optionally disable these dynamic controls.

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

  * If less than or equal to the average, the concurrency limit will be
    increased to the current concurrency plus one (additive increase)
    once per RTT, up to a maximum of the configured in flight limit.

  * If greater than the average, or the result indicated back pressure
    from the remote server, the concurrency will be reduced by a factor
    of one half (multiplicative decrease) once per RTT, down to a
    minimum of one.

```rust
impl Service<Request> for ConcurrencyLimit {
    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        match self.limit.permit.poll_acquire(cx, &self.limit.semaphore) {
            Ready(()) => (),
            NotReady => {
                emit!(ConcurrencyLimited);
                return NotReady;
            }
            Err(err) => return Err(err),
        }

        Poll::Ready(ready!(self.inner.poll_ready(cx)))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let future = self.inner.call(request);
        ...
        emit!(ConcurrencyLimit { concurrency: self.limit.maximum() });
        emit!(ConcurrencyActual { concurrency: self.limit.used() });
        ResponseFuture::new(future, self.limit.semaphore.clone(), Instant::now())
    }
}

impl Future for ResponseFuture {
    fn poll(&mut self, cx: &mut Context) -> Poll<Self::Output> {
        match self.inner.poll() {
            Pending => Pending,
            Ready(output) => {
                let now = Instant::now();
                let rtt = now.duration_since(self.start_time);
                emit!(RTTMeasurement { rtt: rtt.as_millis() });

                let mut controller = self.controller.lock();
                if now >= controller.next_update {
                    if rtt > controller.rtt + controller.threshold {
                        // The `+ 1` prevents this from going to zero
                        controller.concurrency_limit = (controller.concurrency_limit + 1) / 2;
                    }
                    else if controller.concurrency_limit < controller.in_flight_limit
                        && rtt <= controller.rtt {
                        controller.concurrency_limit =
                            min(controller.current_concurrency, controller.concurrency_limit) + 1;
                    }
                    controller.next_update = now + controller.measured_rtt.average();
                }
                controller.measured_rtt.update(rtt);

                Ready(output)
            }
        }
    }
}
```

### Observed Behavior

This algorithm should have the following responses to service
conditions:

* Under normal use, the RTT will stay relatively constant or increase
  slightly in proportion with the concurrency. This should allow the
  concurrency to increase slowly to the configured maximum, increasing
  the delivery rate (assuming no limit is reached).

* If a remote service suddenly becomes unresponsive, with sustained
  timeouts, Vector will rapidly reduce the request concurrency down to
  the minimum of one.

* If a remote service gradually increase its response time, Vector will
  gracefully reduce its request concurrency, with it going down to the
  minimum if the response time continues to increase.

* If a remote service has a hard rate limit, expressed with either HTTP
  response 429 or timeouts for example, lower than what Vector has ready
  to deliver to it, Vector's concurrency should hover around `rate_limit
  / RTT`, peeking over and then briefly dropping down when queries are
  limited. This will keep the delivery rate close to the discovered rate
  limit.

* If the sender experiences a sudden increase in volume of events,
  Vector will not overload the remote service with concurrent requests.
  Instead, Vector will use the maximum concurrency previously set, which
  will be at most one higher than the previously observed limit, and
  continue to ramp up to the configured maximum from there.

### Observability

Vector operators need to be able to observe the behavior of this
algorithm to ensure that it is operating as desired. To this end, the
mechanism will expose the following data:

* a counter metric recording every time a request is limited due to the
  current concurrency limit,

```rust
impl InternalEvent for ConcurrencyLimited {
    fn emit_logs(&self) {
        warn!(
            message = "Request limited due to current concurrency limit.",
            concurrency = %self.concurrency,
            component = %self.component,
            rate_limit_secs = 5,
        );
    }
    fn emit_metrics(&self) {
        counter!("concurrency_limit_reached_total", 1,
            "component_kind" => "sink",
            "component_type" => self.component,
        );
    }
}
```

* a histogram metric recording the observed RTTs,

```rust
impl InternalEvent for ObservedRTT {
    fn emit_metrics(&self) {
        timing!("observed_rtt", self.rtt,
            "component_kind" => "sink",
            "component_type" => self.component,
        );
    }
}
```

* a histogram metric recording the effective concurrency limit, and

```rust
impl InternalEvent for ConcurrencyLimit {
    fn emit_metrics(&self) {
        value!("concurrency_limit", self.concurrency,
            "component_kind" => "sink",
            "component_type" => self.component,
        );
    }
}
```

* a histogram metric recording the actual concurrent requests in flight.

```rust
impl InternalEvent for ConcurrencyActual {
    fn emit_metrics(&self) {
        value!("concurrency_actual", self.concurrency,
            "component_kind" => "sink",
            "component_type" => self.component,
        );
    }
}
```

## Prior Art

* [TCP congestion control algorithms](https://en.wikipedia.org/wiki/TCP_congestion_control)
* [Additive Increase/Multiplicative Decrease](https://en.wikipedia.org/wiki/Additive_increase/multiplicative_decrease)
* [Netflix Technology Blog: Performance Under Load](https://medium.com/@NetflixTechBlog/performance-under-load-3e6fa9a60581)
* [JINSPIRED - Adaptive Safety Control (archive.org)](https://web.archive.org/web/20130105023839/https://www.jinspired.com/site/jxinsight-opencore-6-4-ea-11-released-adaptive-safety-control)

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

* [ ] Submit a PR with spike-level code _roughly_ demonstrating the
      change.
* [ ] Expose major concurrency limiting events as rate-limited logs (ie
      explicit limiting responses).
* [ ] Expose statistics of the concurrency management through internal
      metrics' gauges.
* [ ] Benchmark the approach under various conditions to determine a good
      value for α.
* [ ] Develop test harness to ensure desired rate management behavior
      actually happens and will not regress.
