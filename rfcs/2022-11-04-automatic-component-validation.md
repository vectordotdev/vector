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
validation harness called the "validation runner." This runner would construct an isolated Vector
topology that ran the component, and provided all of the necessary inputs/outputs for it to be a
valid topology. By controlling the input events sent to the topology, and collecting the output
events and telemetry, we would ensure thhat the component could be tested in isolation and in a
scenario that looked more like Vector running in a customer's environment than a simple unit test.

This runner would run within a unit test itself, being fed a list of components to validate, as well
as the test cases (set of input events and expected outcome i.e. success vs failure) for that
componment, and would run each test case in isolation, as described above. Any failing test case,
that is to say, the outcome of the test was not valid, would entirely fail the overall test.

Components themselves would be responsible for implementing a new trait that provided the necessary
information to programmatically drive the validation runner, such as the component's configuration,
the test cases to run, and any external resources that must be provided to properly drive the
component.

A command would be added to allow developers to run these validation tests locally, but a CI check
would also be added to ensure the validation tests were run on a binary that resembles an actual
release, such that we would be ensuring the compliance of all components that can be used.

### Implementation

#### Identifying a component as being validatable

All components which seek to be validatable will implement a new trait, `ValidatableComponent`, that
exposes the necessary information for the runner to actually build, run, and validate them. This is
an abridged version of the trait:

```rust
pub trait ValidatableComponent: Send + Sync {
    /// Gets the name of the component.
    fn component_name(&self) -> &'static str;

    /// Gets the type of the component.
    fn component_type(&self) -> ComponentType;

    /// Gets the component configuration.
    fn component_configuration(&self) -> ComponentConfiguration;

    /// Gets the external resource associated with this component.
    fn external_resource(&self) -> Option<ExternalResource>;

    /// Gets the test cases to use for validating this component.
    fn test_cases(&self) -> Vec<TestCase>;
}
```

The `ValidatableComponent` trait exposes the minimum amount of information necessary to build, run,
and validate the component, specifically:

- the name of the component (via `vector_config::NamedComponent`)
- the type of the component itself (source, transform, or sink)
- the component's configuration (an actual object that can be passed to `ConfigBuilder`)
- the external resource the component depends on, if any (transforms have no external dependency,
  but the `http_server` source, for example, depends on an "external" HTTP client to send it events)
- the test cases to run for the component, which is a list of expectation/events tuples, where the
  expectation dictates if the given inputs should all be processed successfully, partial success, or
  all fail to be processed (this drives detection of relevant telemetry during validation)

This trait overlaps heavily with the existing "component config" traits such as `SourceConfig`, in
that they all roughly describe the high-level characteristics of the component, such as allowable
inputs, how to build the component, and so on.

The intent is to generate the implementation of this trait automatically if at all possible, even if
only partially. As noted above, there's significant overlap with the already present traits such as
`SourceConfig`, but they aren't directly deriveable. For example, the `http_server` source has an
implementation of `SourceConfig` where it describes its only resources as a TCP socket at a given
port. This is the resource the source itself needs to listen for HTTP traffic on that port. This is
all fine and good, but the component runner cares about what external resources need to be provided
to drive the functionality of the component, which in this example, is an HTTP client to actually
send requests to the source.

In this case, the external resource is essentially the inverse of whatever is specified at
`SourceConfig::resources`, but even then, it's still missing all of the required information: what's
the protocol/encoding to use for that socket? Beyond that, this pattern generally only applies to
resources that the component needs to run, and doesn't consider resources that the component will
touch, such as the files that will be read from/written to if the component deals with files.

For this reason, the implementation of `ValidatableComponent` won't be able to be automatically
driven by the existing "component config" trait implementations, but future work could take place
where we unify these traits, and add additional metadata to existing resource declarations.

#### External resource definitions

Arguably the most important part of the `ValidatableComponent` trait, **external resources** define
what resource the runner must provide in order to functionally drive the component under test. At a
high level, an external resource is a type of resource (network socket, files on disk, etc), which
direction the resource operates in (is it pushing to the component? is the component pulling from
it), and the payload type that the resource deals with (JSON, plaintext, etc):

```rust
pub enum ResourceCodec {
    Encoding(EncodingConfig),
    EncodingWithFraming(EncodingConfigWithFraming),
    Decoding(DecodingConfig),
    DecodingWithFraming(DecodingConfig, decoding::FramingConfig),
}

pub enum ResourceDirection {
    Push,
    Pull
}

pub enum ResourceDefinition {
    Http(HttpConfig),
    Socket(SocketConfig),
}

pub struct ExternalResource {
    definition: ResourceDefinition,
    direction: ResourceDirection,
    codec: ResourceCodec,
}
```

As mentioned above, in many cases, external resources are simply the inverse of the declared
resources of a component via their component-specific configuration trait, such as
`SourceConfig::resources`. However, they need more detail for the runner to properly spawn such a
resource, which involves the directionality of the resource and the payloads flowing through it.

Directionality here covers the fact that, for both sources and sinks, they may pull or push with
regards to their external resource. A source could "pull" data from an external resource
(`prometheus_scrape` source), or it could have that external resource "push" to it (`socket`
source), whereas a sink might "push" data to the external resource (`elasticsearch` sink) or have
the external resource "pull" data from it (`prometheus_exporter` sink). Sources and sinks, when
defining their external resource, will define the direction that the external resource operates in
-- push or pull -- which is then further refined depending on whether or not the component is a
source or sink.

As an example, for the `http_server` source, it defines an external HTTP resource with a direction
of "push", which in the context of a source -- where the component gets its _input_ from the
external resource -- means that the resource should "push" the input events to the component, which
in this case, implies an HTTP client that sends inputs. In the case of the `elasticsearch` sink, it
would also describe an external HTTP resource with a direction of "push", but when evaluated in the
context of the component being a sink, this would dictate that we spin up an HTTP server to receive
the output events, as the component is the one doing the "pushing."

Beyond direction, external resources also dictate the type of payload. Handling payloads is another
tricky aspect of this design, as we need a generic way to specify the type of payload whether we're
handling it in a raw form, such as the bytes we might send for it over a TCP socket, or the internal
event form, such as if we wanted to generate a corresponding `Event` to pass to a transform. We also
have to consider this translation when dealing with external resources and wanting to validate what
the component received/sent: if we need to send a "raw" payload to a source, and then validate we
got the expected output event, that means needing to compare the raw payload to the resulting
`Event`.

As such, components will define their expected payload by using the existing support for declaring
codecs via the common encoding/decoding configuration types, such as `EncodingConfig`,
`EncodingConfigWithFraming`, and `DecodingConfig`. This allows components to use their existing
encoding/decoding configuration directly with the runner, which ensures we can correct encode input
events, and decode output events, when necessary.

We'll provide batteries-included conversions from the aforementioned types into `ResourceCodec`.
`ResourceCodec` is intended to work seamlessly with either an encoding or decoding configuration: it
will be able to generate an inverse configuration when necessary. This is what will allow a sink,
with an encoding configuration, to have an external resource generated that knows how to _decode_
what the sink is sending. While the existing encoding/decoding enums --  where we declare all the
standard supported codecs -- do not have parity between each other to seamlessly facilitate this,
we'll look to remedy that situation as part of the work to actually implement support for each of
these codecs. There's no fundamental reason why all of the currently supported encodings cannot have
reciprocal decoders, and vise versa.

#### Test cases

**Test cases** provide the mechanism to craft the set of input events towards a desired outcome of
success, failure, or partial success.

Test cases encode two main pieces of information: the expected outcome, and the input events that
when processed, should lead to the expected outcome. This data is straightforward in practice, but
we've layered an additional mechanism on top to help more easily define input events such that they
can be mutated as necessary to achieve the intended outcome.

Let's briefly look at some code:

```rust
// Expected outcome of a validation test case.
pub enum TestCaseExpectation {
    /// All events were processed successfully.
    Success,

    /// All events failed to be processed successfully.
    Failure,

    /// Some events failed to be processed successfully.
    PartialFailure,
}

/// An event used in a test case.
pub enum TestEvent {
    /// The event is used, as-is, without modification.
    Passthrough(Event),

    /// The event is potentially modified by the external resource.
    Modified(Event),
}

/// A validation test case.
pub struct TestCase {
    pub expectation: TestCaseExpectation,
    pub events: Vec<TestEvent>,
}

```

As described above, these types are simple in practice, but provide the means to articulate changes
to the input events necessary for inducing failure. Let's walk through a simple example using the
`http_server` source.

In the most basic test case, we can trivially provide simple log events that will be encoded and
processed by the source without issue. However, in order to exercise the failure path, we need to
consider what causes the source to fail to process an event.

For the `http_server` source, the only real failure path is a payload that cannot be decoded. From
the definitions we've seen above, the only control that we really have is over the contents of the
event itself. As components describe their own configuration, and the configuration of their
external resource, in a fixed way, this means that we could, for example, only specify the external
resource to create with a fixed codec, such as JSON. The HTTP resource code is configured with this
information, and will dutifully take an `Event` and encode it to JSON before sending it along.

To get around this, `TestEvent` is introduced and provides two modes: passthrough and modified. The
passthrough mode is exactly what it sounds like: the event it holds is used as-is. When faced with a
situation such as our example, where modifying the event itself cannot be used to induce a failure,
we can utilize the modified mode to do so.

The modified mode also wraps an `Event`, but it is used as a signal to an external resource that the
external resource should modify the event in some context-specific way in order to go against the
happy path. As you might surmise, this is generally only useful for sources, because transforms and
sinks will always be given an `Event`, and so the event itself must be tailored to induce failure in
thos cases.

Going back to our example, we can craft a "modified" event which will instruct the HTTP client
external resource to encode the event using an encoding that _isn't_ the one it was configured with.
Codifying this in the definition of the test cases for the `http_server` source would look something
like this:

```rust
impl ValidatableComponent for SimpleHttpConfig {
    ...

    fn test_cases(&self) -> Vec<TestCase> {
        vec![
          // This is a happy path test, and each event will be encoded according to the external resource definition.
          TestCase::success(vec![
            LogEvent::from_str_legacy("simple message 1"),
            LogEvent::from_str_legacy("simple message 2"),
            LogEvent::from_str_legacy("simple message 3"),
          ]),

          // This is a failure path test, where we've marked one event such that the external resource should modify
          // it, which in this case will mean it gets encoded using a codec other than the one specified in the
          // external resource definition.
          TestCase::partial_failure(vec![
            LogEvent::from_str_legacy("good message 1").into(),
            TestEvent::modified(LogEvent::from_str_legacy("bad message 1")),
            LogEvent::from_str_legacy("good message 2").into(),
          ]),
        ]
    }
}
```

As you can see from the above, there are also many implicit conversions provided so that developers
can write test cases with a minimal amount of type conversion boilerplate, focusing as much as
possible on the content of the events, and their shape, needed to drive the intended outcome.

#### Pluggable validators

With the ability to build an arbitrary component, as well as the ability to build any external
resource it depends on, we also need to define what validation of the component actually looks like.
This is where **validators** come into play.

Validators are simple implementations of a new trait, `Validator`, that are able to collect
information about the component before and after the validation run is completed, and be transformed
into more human-friendly output to be shown at the conclusion of a component validation run, whether
the component is in compliance or discrepancies were observed.

Let's take a brief look at `Validator` trait:

```rust
pub trait Validator {
    /// Gets the unique name of this validator.
    fn name(&self) -> &'static str;

    /// Processes the given set of inputs/outputs, generating the validation results.
    fn check_validation(
        &self,
        component_type: ComponentType,
        expectation: TestCaseExpectation,
        inputs: &[TestEvent],
        outputs: &[Event],
        telemetry_events: &[Event],
    ) -> Result<Vec<String>, Vec<String>>;

}
```

Validators are added to a runner before the component is validated, and called after a component
topology has finished, and all outputs and telemetry events have been collected.

Each validator is provided with essentially all relevant information about the test case run: the
component type, the expected outcome of the test case, the input events, the output events, and all
telemetry events from the run. This set of information meets the baseline set of requirements for
the two most important validators: the Component Specification validator, and the Metrics
Correctness validator.

The validation runner itself is responsible for collecting all of this information and providing it
to the validator, rather than the validator having to collect it for itself. As a benefit, this
means that validators themselves could also be unit tested rather easily, rathering than having to
mutate global state, or singleton objects, to ensure their logic is correctly driven.

#### The component runner

Finally, with all of these primitives, we need a way to bring them all together, which is handled by
the component runner, or `Runner`. This type brings all of the pieces together -- the component to
validate, the validators to run -- and handles the boilerplate of building the component to validate
and any external resource, generating the inputs, sending the inputs, collecting the outputs, and
actually driving the component and external resource until all outputs have been collected.

Here's a simple example of how the runner would actually be used to bring everything together:

```rust
fn get_all_validatable_components() -> Vec<&'static dyn ValidatableComponent> {
    // Collect all validatable components by using the existing component registration mechanism,
    // `inventory`/`inventory::submit!`, to iterate through sources, transforms, and sinks, collecting
    // them into a single vector.
}

#[test]
async fn compliance() {
    let validatable_components = get_all_validatable_components();
    for validatable_component in validatable_components {
        let component_name = validatable_component.component_name();
        let component_type = validatable_component.component_type();

        let mut runner = Runner::from_component(validatable_component);
        runner.add_validator(StandardValidators::ComponentSpec);

        match runner.run_validation().await {
            Ok(test_case_results) => {
                let mut details = Vec::new();
                let mut had_failures = false;

                for (idx, test_case_result) in test_case_results.into_iter().enumerate() {
                    for validator_result in test_case_result.validator_results() {
                        match validator_result {
                            Ok(success) => {
                                // A bunch of code to take the success details and format them nicely.
                            }
                            Err(failure) => {
                                had_failures = true;

                                // A bunch of code to take the failure details and format them nicely.
                            }
                        }
                    }
                }

                if had_failures {
                    panic!("Failed to validate component '{}':\n{}", component_name, details.join(""));
                } else {
                    info!("Successfully validated component '{}':\n{}", component_name, details.join(""));
                }
            }
            Err(e) => panic!(
                "Failed to complete validation run for component '{}': {}",
                component_name, e
            ),
        }
    }
}
```

In this example, we can see that we create a new `Runner` by passing it something that implements
`ValidatableComponent`, which in this case we get from the existing `&'static` references to
components that have registered themselves statically via the existing
`#[configurable_component(<type>("name"))]` mechanism, which ultimately depends on `inventory` and
`inventory::submit!`.

From there, we add the validators we care about. Similar to codecs, we'll have a standard set of
validators that should be used, so the above demonstrates a simple enum that has a variant for each
of these standard validators, and we use simple generic constraints and conversion trait
implementations to get the necessary `Box<dyn Validator>` that's stored in `Runner`.

With the validators in place, we call `Runner::run_validation`, which processes every test case for
the component. For each test case, we spin up all of the necessary input/output tasks, any external
resource that is required, and build a component topology which is launched in its own isolated
Tokio runtime. The runner drives all of these tasks to completion, and in the right order, and
collects any output events and telemetry events.

Our validators are called at the end of the test case run to check the validity of the component
based on the input events, collected outputs, and so on. The validators can provide rich textual
detail about the success or failure of the given test case run for the component.

All of the test cases are run, and the results collected, and checked at the end to determine
whether or not the component is valid against the configured validators. Any success or failure
detail is emitted at this point to inform the developer of the result, and indicate what aspects
were not valid, and so on.

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

The main drawback of this work is the additional boilerplate required for each component. While not
particularly onerous, it does represent another bit of code that looks and feels a lot like
boilerplate that already required, such as `SourceConfig`, and so on.

Additionally, this proposal does run the component topology within the same process, which means
that we have to spend time to make sure that the internal telemetry is properly isolated so that
different validation runs don't bleed into each other, or that internal telemetry generated by code
used in the validation runner itself doesn't leak into the telemetry events collected for the
component. This is achieveable but represents a small amount of additional work needed to
successfully use this approach.

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

One alternative approach could be to actually build Vector and run an entire configuration, much
like an integration test, and we would send known data to Vector, collect the output, so on and so
forth, much like this proposal... but we would simply collect it externally, much like `lading` does
in our soak tests. Said another way, replace the part of `Runner` that builds a `RunningTopology`
from a `ConfigBuilder`... and simply generate a real Vector configuration and instead spawn a Vector process.

This approach does solve the issue of isolation of telemetry data rather concretely, as each test
case would be a new Vector process, with no risk whatsoever of contaminated telemetry events between
test cases. It also potentially opens up the ability to provide full-fledged configuration files
where the proposed traits/interfaces are suboptimal and would require complex refactoring to support
edge cases.

The biggest reason not to go with this approach is that we lose some measure of programmatic control
over running the Vector topology:

- we would lose the ability to use in-application synchronization primitives for determining when
  components/tasks were ready (such as whether or not a component was ready, relegating us to trying
  to, for example, connect to a TCP socket until we finally got a connection)
- determining/collecting errors could be harder as we would be forced to parse the stdout/stderr of
  the Vector process in some cases to understand issues with the configuration

## Outstanding Questions

- Is the external resource definition approach flexible enough to cover all possible definitions of
  an external resource? We know it works for simple use cases like "network socket w/ a specific
  encoding", but will that extrapolate cleanly to all components that we'll need to support?
- Can we fully represent all possible external resources in general? For example, can we actually
  provide a Kafka broker meaningfully without having to, say, run an actual copy of Kafka
  (regardless of whether or not it was through Docker, etc)? Do we care that much if we end up
  having to actually spin up some external resources in that way versus being able to mock them all
  out in-process?

## Plan Of Attack

- [ ] Merge the RFC PR, which contains the proof-of-concept approach as described in the RFC itself.
- [ ] Implement a Component Specification validator: check for expected events/metrics based on
  component type.
- [ ] Implement a Metrics Correctness validator: check that the metric emitted for bytes received
  by a source matches the number of bytes we actually sent to the source, and so on.
- [ ] Implement more external resources: raw socket, specific services that can be emulated (i.e.
  Elasticsearch is just HTTP with specific routes). This would likely be split into multiple items
  after taking an accounting of which unique external resource variants need to exist.
- [ ] Implement `ValidatableComponent` for more components, adding each component to the hard-coded
  "validate these components" list prior to making it "all or nothing". This, likewise, will be able
  to be split into a per-component set of tasks as implementation of `ValidatableComponent` for one
  component should be highly localized.
- [ ] Update the implementation of the "get all validatable components" method to source all
  components from their respective `inventory`-based registration. This would effectively turn on
  validation of any component that is compiled into the Vector binary and is configurable (i.e. it
  uses `#[configurable_component(...)]`)

## Future Improvements

- Add additional validators to validate other aspects of the component: high-cardinality detection,
  functional validation, etc.
- Drive the generated inputs in a manner similar to property testing, allowing the validation
  process to potentially uncover scenarios where valid inputs should have been processed but
  weren't, or a certain metric's value wasn't updated even though the component acknowledged
  processing an event, and so on.
- Try and unify the distinct per-component configuration traits (i.e. `SourceConfig`) with
  `ValidatableComponent` into a more generic base trait.
