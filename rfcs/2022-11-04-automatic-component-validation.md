# RFC 15103 - 2022-11-04 - Automatic component validation

Over time, the Vector team has undertaken numerous attempts to ensure that the internal telemetry
emitted from Vector, specifically the internal telemetry related to components, is both accurate and
consistent. This RFC charts a path for an external testing mechanism that will ensure that if a
component can be used/configured by an end user, it is tested thoroughly, and has passed all of
the relevant qualifications --
[Component Specification](https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md)
adherence, correctness of actual metric values, etc -- that make it suitable for production use.

## Context

- [#8858](https://github.com/vectordotdev/vector/pull/8858) - chore: Add component specification
- [#9308](https://github.com/vectordotdev/vector/pull/9308) - chore: Add script to surface-scan
  components against specification
- [#9312](https://github.com/vectordotdev/vector/pull/9312) - enhancement(unit tests): Add testing
  for component specification features
- [#9687](https://github.com/vectordotdev/vector/issues/9687) - Audit sources to ensure compliance
  with the component spec
- [#9688](https://github.com/vectordotdev/vector/issues/9688) - Audit sinks to ensure compliance
  with the component spec
- [#10832](https://github.com/vectordotdev/vector/issues/10832) - Update component instrumentation
  for multiple outputs
- [#10882](https://github.com/vectordotdev/vector/issues/10882) - Vector Internal Metrics has no
  data for `component_errors_total`
- [#11342](https://github.com/vectordotdev/vector/issues/11342) - Ensure all existing metric `error`
  tags comply with updated component spec
- [#12492](https://github.com/vectordotdev/vector/pull/12492) - enhancement(datadog service): add
  bytes sent metrics to Datadog sinks
- [#12572](https://github.com/vectordotdev/vector/pull/12572) - chore(observability): ensure all
  sources are checked for component spec compliance
- [#12668](https://github.com/vectordotdev/vector/pull/12668) - chore(observability): ensure
  transforms are checked for component spec compliance
- [#12755](https://github.com/vectordotdev/vector/pull/12755) - chore(observability): ensure sinks
  are checked for component spec compliance
- [#12768](https://github.com/vectordotdev/vector/pull/12768) - chore(observability): make
  `RequestBuilder` provide (un)compressed payload sizes
- [#12910](https://github.com/vectordotdev/vector/pull/12910) - chore(vector source): fix the
  `BytesReceived` event for v2 variant
- [#12964](https://github.com/vectordotdev/vector/issues/12964) - bug(blackhole sink): does not
  report "network" bytes sent
- [#13995](https://github.com/vectordotdev/vector/issues/13995) - Verify
  `component_discarded_events_total` and `component_errors_total` for all sources/sinks
- [#14073](https://github.com/vectordotdev/vector/issues/14073) - verify `route` transform's
  `component_discarded_events_total`

## Scope

### In scope

- Validating components against the
  [Component Specification](https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md)
  in terms of both events emitted as well as metrics emitted.
- Validating the correctness of metric values: if a source is sent 100 bytes in a request, we
  properly increment the "bytes received" metric by 100 bytes, etc.

### Out of scope

- Validating a component against all possible permutations of its configuration.
- Validating functional behavior of a component i.e. a transform that should lowercase all strings
  actually lowercased all strings, etc.
- Validating any component types besides sources, transforms, and sinks.

## Pain

Currently, there is a significant amount of pain around knowing that a component emits the
intended/relevant telemetry for that component, and that the values emitted are correct. As
developers providing support, and operators actually running Vector, the ability to build dashboards
and alerting that work consistently for different component types is of the utmost importance.

The current process of validating components is inherently manually: we can write a unit test to
check for compliance, but if we forget to write that unit test, or don't update all unit tests when
we want to change the behavior for, say, all sources... then our compliance checking can quickly
become out-of-sync with reality.

This goes double for commuinity contributions: while the Vector authors might have learned over time
to make sure they're emitting the right metrics, and so on, it's essentially impossible for a
first-time contributor to know to do so unless there is some mechanism in place to tell them to do
so.

Humans are also inherently fallible: it is easy to look at data and misread a number, to think
something is there when it isn't. This has occurred multiple times -- as evidenced by the multiple
issues linked in the **Context** section -- during work to validate components, despite multiple
engineers participating in the work, each of them giving it a fresh look.

## Proposal

### User Experience

Instead of having to manually add unit tests to an individual component, components would be
automatically tested if they were usable. Being "usable", in this case, means that the component is
present in a given Vector binary: if we're building Vector in a way that includes, say, component X,
its mere inclusion in the binary will ensure that component X gets tested. It should effectively be
impossible to include a component in a build in a way that doesn't surface it to be tested.

These components would be validated for compliance by being run automatically in a tailored
validation harness called the "validation runner." This external process would construct a Vector
configuration such that the desired component was part of the topology, and it would run Vector with
this configuration. By controlling the input events, and collecting the output events and telemetry,
we would ensure thhat the component could be tested in isolated and in a scenario that looked more
like Vector running in a customer's environment than a simple unit test.

This runner would itself be invoked as an external process, similar to soak tests, and driven by two
mechanisms: the output of `vector list` and individualized test configuration data. Using
`vector list` would ensure that all components present in the Vector binary were tested, and using
individualized test configuration data -- think small JSON/YAML files for each test scenario, again,
like soak tests -- would allow us to avoid modifying existing source code, as well as providing the
ability to specify more free-form configuration data without tight/rigid coupling to the source.

A command would be added to allow developers to run these validation tests locally, but a CI check
would also be added to ensure the validation tests were run on a binary that resembles an actual
release, such that we would be ensuring the compliance of all components that can be used.

### Implementation

#### Identifying what components to validate

The `vector list` subcommand already exists, which provides an excisting entrypoint into figuring
out which components to actually validate. This mechanism is already driven by the existing
"configurable" infrastructure, such that if a component is annotated in a way to make it usable in a
Vector configuration, it will also always be present in the output of `vector list`.

#### Component configuration and event payloads

While knowing which components to validate is obviously a requirement, the other important part is
knowing how to run the component and in what way to drive it for validation. Validating a component
implies checking that it does what it supposed to do, and ostensibily, a component handles events in
some way, which means feeding those events as part of a validation run.

In order to solve this particular issue, components will specify individiaulized test configuration
data in the form of on-disk files, like so:

```
validation/components/sources/http_server/basic.yaml
validation/components/sources/http_server/advanced.yaml
validation/components/transforms/remap/basic.yaml
...
```

These files provide the three main sources of information mentioned above: how to configure the
component, how to drive that component, and what, if any, events it must be fed, such as data in a
specialized format, and so on.

The layout of these files would specify the configuration of the component, how to drive it,
and the input events. The component configuration would be identical to component's normal
configuration schema. The "driving" configuration would look somewhat like an inverse of the
component configuration. The input events data would be a simple-but-accessible schema that should
be very obvious to any user who works with Vector. Here's an example of what this could look like:


```yaml
component:
  type: source
  source:
    type: http_server
    address: 0.0.0.0:80
    decoding:
      codec: json
runner:
  external:
    type: http_client
    args:
      address: 0.0.0.0:80
      method: POST
      encoding:
        codec: json
  events:
    allowed_types:
      - log
```

This configuration represents a simple test of the `http_server` source component. We specify the
component's type -- a source -- along with the actual component's configuration, under `component`.
This is used to not only build the Vector configuration itself, but also to inform the runner about
how to interpret the configuration of the external resource.

The external resource configuration represents the "mock" that the validation runner will provide in
order to drive events into Vector, or in some cases, get events out. It will generally represent the
inverse the component's configuration: if we're testing a source that listens for events on a
socket, then the external resource will be a client that pushes events over a socket, and if we're
testing a sink that exposes an HTTP server to have events scraped from it, then the external
resource will be an HTTP client that scrapes the events from the HTTTP server.


Finally, we specify the event types that this component allows. Since we're only sending events that
the component allows, this implies the test is a "happy" path test, since we're theoretically only
sending it events it will accept, with a configuration that matches (in terms of codecs and so on).

As mentioned above, this approach allso allows us the ability to specify other bits of information,
such as bad events, and so on, that could be used to validate a failure path. Here's an example of
what that could look like:

```yaml
component:
  type: sink
  sink:
    type: http
    uri: localhost:1234
    encoding:
      codec: json
    # We'll use the wrong HTTP method which should lead to HTTP errors when the
    # sink is pushing events downstream,
    method: PUT
runner:
  # We would annotate the runner configuration to specify that this test
  # should result in all errors.. aka the "failure" path.
  test_type: failure
  external:
    type: http_server
    args:
      address: localhost:1234
      method: POST
      encoding:
        codec: json
  events:
    allowed_types:
      - logs
```

Perhaps, however, we need to actually tailor the events themselves in order to induce failure, which
could be necessary if we're dealing with components that expect events to match a given schema
definition, or have some sort of data constraint i.e. a field has a string value that matches a
pattern, or a numeric value that falls within a certain range, and so on. Perhaps we want to test
something a little more functional, like the `filter` transform, and if it's properly emitting
metrics when we give it events that should be filtered. We could imagine such a configuration might
look like this:

```yaml
component:
  type: filter
  condition: .foo == "a"
runner:
  component_type: transform
  events:
    fixed:
      - event_type: log
        outcome: dropped
        fields:
          foo: a
      - event_type: log
        fields:
          foo: b
      - event_type: log
        fields:
          bar: a
```

This would configure the the validation runner to send three log events, only one of which would
have the field/field value to trigger the filter condition. We annotate that event with the expected
outcome of that event being processed by the component being validated, so this runner would know
that we should send in three events -- as defined above -- and get back only two: the event with
`foo = b` and the event with `bar = a`... with the respective metrics set for having dropped an
event, and so on.

These various forms of getting away from defaults -- if the test case is the happy path, or if we
expect errors, exactly what events to send and their expected outcome, etc -- would apply
generically whether the component being validated was a source, transform, or sink. We would use the
information in whatever way was necessary based: if fixed events were specified for the example
`http_server` source validation test, we would simply encode them as we were configured to, and so
on.

#### Generation of a valid and correct Vector configuration

We talked above about how a developer would specify the configuration for the component being
validated, but one aspect we left out is generating the _other_ portions of the configuration, as we
would need a full configuration -- source to sink at the minimum, or source to transforms to sink if
testing a transform -- in order for Vector to actually run.

The validation runner would fill in whatever missing pieces existed based on our "native" Vector
components: the `vector` source or `vector` sink, depending on the component being validated. For
example, if we were testing a source, we would create a `vector` sink as the output mechanism, which
the validation runner would connect to in order to receive output events. Likewise, if we were
testing a sink, we would use the `vector` source in order to feed input events. For transforms, we
would use both the `vector` source and `vector` sink.

Additionally, all generated configurations would use the `internal_events` and `internal_metrics`
sources, emitting via a dedicated `vector` sink, in order to actually capture the relevant events
and metrics that were being validated.

#### External resources

As mentioned above, we would create mocks/instance of external resources, regardless of what their
type was. This means that we may create an "HTTP server" external resourced by creating a
`hyper`-based implementation in the validation runner itself. This would scale for most external
resources that were network-based -- HTTP, raw sockets, etc -- as well as file-based.

For some components, mocking their external resources entirely in the runner process would be
non-trivial, such as doing so for Kafka. Obviously, Kafka is itself is "network-based" in terms of
interacting with it, but there exists no full in-memory implementation of Kafka that we could run.
This likely applies to some other components that we simply have not fully considered yet under the
spotlight of this proposal.

For these components, we would likely have to adopt an integration test-style approach of actually
allowing the test configuration to specify the external resource in terms of running a process
directly, whether it was a binary on the host system, or a Docker container to spawn, and so on.

Beyond those cases, we would simply look to create implementations of the various external resources
that the validation runner itself would run and manage, in order to keep the number of _actual_
external dependencies -- in terms of the validation runner -- as low as possible.

#### Pluggable validators

With the ability to build an arbitrary component, as well as the ability to build any external
resource it depends on, we also need to define what validation of the component actually looks like.
This is where **validators** come into play.

Validators are simple implementations of a new trait, `Validator`, that are able to collect
information about the component before and after the validation run is completed, and be transformed
into more human-friendly output to be shown at the conclusion of a validation run, whether
the component is in compliance or discrepancies were observed.

Let's take a brief look at `Validator` trait:

```rust
pub trait Validator {
    /// Gets the unique name of this validator.
    fn name(&self) -> &'static str;

    /// Runs the pre-hook logic for this validator.
    fn run_pre_hook(&mut self, component_type: ComponentType, inputs: &[Event]) {}

    /// Runs the post-hook logic for this validator.
    fn run_post_hook(&mut self, outputs: &[Event], events: &[JsonValue], metrics: &[Metric]) {}

    /// Consumes this validator, collecting and returning its results.
    fn into_results(self: Box<Self>) -> Result<Vec<String>, Vec<String>>;
}
```

Validators are added to a runner before the component is validated, but their lifecycle, as briefly
described above, looks like the following:

- right before a component is validated, but after we've generated the input payloads to be used,
  `Validator::run_pre_hook` is called, getting both the component type as well as a list of the
  input payloads that will be sent
- immediately after the component has been run and all output payloads have been collected,
  `Validator::run_post_hook` is run, getting a list of all output payloads that were collected, as
  well as any internal events (log messages) and metrics
- finally, each validator is transformed into a `Result`-based human-friendly validation output via
  `Validator::into_results`, which will generally be nothing in the success/`Ok(...)` case, but will
  generally include human-friendly error messages in the failure/`Err(...)` case, such as which item
  did not pass validation, and so on

We provide the inputs and outputs but not all components will actually generate an output for each
input -- transforms being the simplest example of where there's not always a one-to-one mapping --
so providing the output events is primarily for basic checking or calculation of expected values,
such as what the count of "received events" for a component should be, and so on. The real "meat" of
the validation run are the internal events (log messages) and internal metrics that were emitted.

#### The validation runner

Finally, with all of these primitives, we need a way to bring them all together, which is handled by
the validation runner, or `Runner`. This would represent the core logic of the validation run:

- loading all validatable components via `vector list`
- loading the individualized test configurations for each of them
- generating a valid/correct Vector configuration for each test
- configuring any necessary "external resource"
- configuring the listeners for the internal telemetry data
- generating the events to actually send to Vector
- starting Vector, sending all input events, waiting for all output events, and stopping Vector
- running all configured validators and emitting a pass/fail based on their results

## Rationale

Simply put, we need accurate internal telemetry. Vector must be able to provide accurate internal
telemetry for users so that they can properly operate it: whether that's in terms of ensuring
Vector is performing as expected or if users are getting the full benefit of their Vector
deployments, perhaps in terms of cost savings by reducing data egress, and so on.

When we ensure that all components are automatically tested as soon as they're fully added to the
codebase, we can better ensure that not only do new components get properly tested, but that
existing components are still being tested as well.

If we don't do this, or something like this, we'll continue to avoid detecting some metrics that are
very likely already incorrect and have evaded our attempts to discover them so far. Additionally,
we're likely to once again introduce regressions to components, and miss issues with new components,
leading to a permanent trail of subtly-wrong internal telemetry.

## Drawbacks

The main drawback of this work is the additional data required per component that is not coupled to
source code: this represents another potential avenue of drift between the component as it is
implemented/described in Rust source vs how it is described in the test configuration data.

## Prior Art

Vector already has existing mechanisms for validating components in terms of the Component
Specification, and so on. This comes in the form of familiar helper methods like
`assert_sink_compliance`, `assert_source_error`, etc. Those helpers operate with a similar approach
to this proposal in that we run the relevant code in a way that ensures we can observe the state of
the system before and after the code runs, calculating the validation results at the end.

We do not have an existing mechanism for asserting the _correctness_ of internal telemetry, only
that specific bits of internal telemetry are being emitted as intended. This is not necessarily a
technical limitation, but a practical limitation of not yet having gotten to the point of adding
support to validate the correctness.

What both of these things do not provide, that this proposal does, is a mechanism to _ensure_ that a
component that can be used has been actually tested against the intended criteria/specifications/etc
and met them. While this doesn't happen as a result of the compiler/type system, and so is still
somewhat fallible at the edges -- we have to actually do work to integrate this proposal with CI,
etc -- it achieves uniform coverage across all components and provides the scaffolding to avoid the
human fallibility aspect of "oops, we're not actually validating this component".

It also heavily centralizes this validation logic such that any future work around updating what
gets validated, or exposing more information during the validation step, and so on, can happen as a
set of changes to a relatively small set of files, rather than, again, a change to every component
involved, which is exactly where human fallibility has struck us in the past.

## Alternatives

One alternative approach could be to implement such a system but internal to Vector itself, as a
unit test/unit test-like mechanism. Instead of an external process creating a Vector configuration
from test configuration files on disk, components could self-describe (likely through a new trait,
or additions to existing traits) the same information -- external resources, etc -- which would be
queried when the test mechanism ran. The unit test mechanism would simply create the component
directly, much like how we create components in the topology builder after parsing the
configuration, and then run them directly in the unit test.

This would provide more of a compile-time-esque feedback mechanism, as the configuration of a
component -- allowed event types, external resource, etc -- could be strongly-typed. We could even,
potentially, derive some of the information automatically. Remember back that the definition of an
external resource often represents the inverse of the component's configuration, and so there could
be the potential to provide helpers/refactor existing code slightly so that the configuration of the
external resource was a trivial data transformation of the component configuration itself.

## Outstanding Questions

- Is the external resource definition approach flexible enough to cover all possible definitions of
  an external resource? We know it works for simple use cases like "network socket w/ a specific
  encoding", but will that extrapolate cleanly to all components that we'll need to support?
- Is the `Validator` trait hooked in enough? We know that it covers the ability to capture events
  and metrics emitted, which is sufficient for validating against the Component Specification, as
  well as validating metrics correctness... but are there other aspects of a component that we plan
  on validating, or wish we could validate, that would not be able to be written based on the
  proposed design?

## Plan Of Attack

- [ ] Create a new crate for the validatio runner: `vector-validation`.
- [ ] Implement a test configuration data parser that can generate a Vector configuration based on
  the described constraints above: valid configuration, fill in all other required components,
  expose internal telemetry, etc.
- [ ] Implement the validation runner "discovery" logic: how to find components to validate, along
  with their test configuration data. Initially, this would be fallible, only running validation
  tests for components that had test configuration data specified.
- [ ] Implement a Component Specification validator: check for expected events/metrics based on
  component type.
- [ ] Implement a Metrics Correctness validator: check that the metric emitted for bytes received
  by a source matches the number of bytes we actually sent to the source, and so on.
- [ ] Implement common external resources: raw socket, specific services that can be emulated (i.e.
  Elasticsearch is just HTTP with specific routes). This would likely be split into multiple items
  after taking an accounting of which unique external resource variants need to exist.
- [ ] Implement the core runner logic, such that we could manually build the validation runner,
  provide a component to be validated, and it would actually go through the motions of validating
  it. (This implies the above steps are completed as least as far as having the external resource
  implemented for whatever component we choose, along with defining its test configuration data,
  etc.)
- [ ] Create a new make target to run the validation tests locally.
- [ ] Implement more of the required external resources and begin adding test configuration for all components.
- [ ] Update the implementation of the "get all components to validate" method to be opt-out: all
  components from `vector list` would be considered to be in scope for validation unless they opted
  out by having a special file on disk where their test configuration data would live. This would
  effectively require validation for all components _unless_ they opted out manually.
- [ ] Add a new step to our test CI workflow to start running the validation steps, closing the loop
  on "if a component is exposed and configurable by users, it must be tested for validation".

## Future Improvements

- Add additional validators to validate other aspects of the component: high-cardinality detection,
  functional validation, etc.
