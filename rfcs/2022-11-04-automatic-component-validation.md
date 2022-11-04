# RFC 15103 - 2022-11-04 - Automatic component validation and verification

Over time, the Vector team has undertaken numerous attempts to ensure that the internal telemetry
emitted from Vector, specifically the internal telemetry related to components, is accurate,
consistent, and correct. This RFC charts a path for a unit test-based mechanism that will ensure
that if a component can be used/configured by an end user, it is unit tested thoroughly, and has
passed all of the relevant qualifications -- Component Specification adherence, correct of actual
metric values, etc -- that make it suitable for production use.

## Context

- [#8858](https://github.com/vectordotdev/vector/pull/8858) - chore: Add component specification
- [#9308](https://github.com/vectordotdev/vector/pull/9308) - chore: Add script to surface-scan components against specification
- [#9312](https://github.com/vectordotdev/vector/pull/9312) - enhancement(unit tests): Add testing for component specification features
- [#9687](https://github.com/vectordotdev/vector/issues/9687) - Audit sources to ensure compliance with the component spec
- [#9688](https://github.com/vectordotdev/vector/issues/9688) - Audit sinks to ensure compliance with the component spec
- [#10832](https://github.com/vectordotdev/vector/issues/10832) - Update component instrumentation for multiple outputs
- [#10882](https://github.com/vectordotdev/vector/issues/10882) - Vector Internal Metrics has no data for `component_errors_total`
- [#11342](https://github.com/vectordotdev/vector/issues/11342) - Ensure all existing metric `error` tags comply with updated component spec
- [#12492](https://github.com/vectordotdev/vector/pull/12492) - enhancement(datadog service): add bytes sent metrics to Datadog sinks
- [#12572](https://github.com/vectordotdev/vector/pull/12572) - chore(observability): ensure all sources are checked for component spec compliance
- [#12668](https://github.com/vectordotdev/vector/pull/12668) - chore(observability): ensure transforms are checked for component spec compliance
- [#12755](https://github.com/vectordotdev/vector/pull/12755) - chore(observability): ensure sinks are checked for component spec compliance
- [#12768](https://github.com/vectordotdev/vector/pull/12768) - chore(observability): make `RequestBuilder` provide (un)compressed payload sizes
- [#12910](https://github.com/vectordotdev/vector/pull/12910) - chore(vector source): fix the `BytesReceived` event for v2 variant
- [#12964](https://github.com/vectordotdev/vector/issues/12964) - bug(blackhole sink): does not report "network" bytes sent
- [#14073](https://github.com/vectordotdev/vector/issues/14073) - verify `route` transform's `component_discarded_events_total`

## Cross cutting concerns

- Link to any ongoing or future work relevant to this change.

## Scope

### In scope

- Validating components against the [Component Specification](https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md)
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

These components would be validated for compliance by being run automatically in a tailored test
harness called the "runner." This runner will be able to take any eligible component -- source,
transform, or sink -- and build an instance of it that we would then run. The runner would control
the input events, as well as any external resource -- HTTP server that gets scraped, downstream TCP
socket that gets sent to, etc -- such that we would know exactly what events were sent as input and
what events were received as output. Additionally, based on the structure of the runner, we would be
able to isolate and capture any events/metrics emitted by the component, and have a full accounting
of the related internal telemetry at the end of a validation run.

This runner would itself be invoked as a normal unit test in Vector, doing a validation run for any
component currently contained in the test binary. A command would be added to allow developers to
run these validation tests locally, but a CI check would also be added to ensure the validation
tests were run on a binary that resembles an actual release, such that we would be ensuring the
compliance of all components that can be used.

### Implementation

#### Identifying a component as being validatable

All components which seek to be validatable will implement a new trait, `Component`, that exposes
the necessary information for the runner to actually build, run, and validate them. This is an
abridged version of the trait:

```rust
pub trait Component {
    /// Gets the type of the component.
    fn component_type(&self) -> ComponentType;

    /// Gets the external resource associated with this component.
    fn external_resource(&self) -> Option<ExternalResource>;

    /// Builds the runnable portion of a component.
    async fn build_component(&self, builder_parts: ComponentBuilderParts) -> Result<BuiltComponent, String>;
}
```

The `Component` trait exposes the minimum amount of information necessary to build, run, and
validate the component, specifically:

- the type of the component itself (source, transform, or sink)
- the external resource the component depends on, if any (transforms have no external dependency,
  but the `http_server` source, for example, depends on an "external" HTTP client to send it events)
- the built component that can actually be run as a future, which will generally be the same output of the
  `build` method on the component type-relevant configuration trait (i.e. `SourceConfig::build`)

This trait overlaps heavily with the existing "component config" traits such as `SourceConfig`, in
that they all have a method to actually generate a "built" version of the component (typically a
`Future` value) as well as describe themselves in terms of what inputs are allowed, or what unique
resources they depend on such as socket ports, or files on disk, and so on.

The intent is to generate the implementation of this trait automatically if at all possible, even if
only partially. As noted above, there's significant overlap with the already present traits such as
`SourceConfig`, but they aren't directly deriveable. For example, the `http_server` source has an
implementation of `SourceConfig` where it describes its only resources as a TCP socket at a given port. This is the resource
the source itself needs to listen for HTTP traffic on that port. This is all fine and good, but the
component runner cares about what external resources need to be provided to drive the functionality
of the component, which in this example, is an HTTP client to actually send requests to the source.

In this case, the external resource is essentially the inverse of whatever is specified at
`SourceConfig::resources`, but even then, it's still missing all of the required information: what's
the protocol/encoding to use for that socket? Beyond that, this pattern generally only applies to
resources that the component needs to run, and doesn't consider resources that the component will
touch, such as the files that will be read from/written to if the component deals with files.

For this reason, the implementation of `Component` won't be able to be automatically driven by the
existing "component config" trait implementations, but future work could take place where we unify
these traits, and add additional metadata to existing resource declarations.

For methods like `build_component`, we can likely provide helper methods to reduce the boilerplate
to a bare minimum, as the code to build a source component, for example, always involves the same
exact steps: have a component object value that implements `SourceConfig`, create a relevant `SourceContext`
value, call `SourceConfig::build` on that component object value with the given source context, and
the result is always the `vector_core::source::Source` type.

#### External resource definitions

Arguably the most important part of the `Component` trait, **external resources** define what resource
the runner must provide in order to functionally drive the component under test. At a high level, an
external resource is a type of resource (network socket, files on disk, etc), which direction the
resource operates in (is it pushing to the component? is the component pulling from it), and the
payload type that the resource deals with (JSON, plaintext, etc):

```rust
pub enum PayloadCodec {
    JSON,
    Syslog,
    Plaintext,
}

pub struct Payload {
    codec: PayloadCodec,
    event_types: DataType,
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
    payload: Payload,
}
```

As mentioned above, in many cases, external resources are simply the inverse of the declared
resources of a component via their component-specific configuration trait, such as
`SourceConfig::resources`. However, they need more detail for the runner to properly spawn such a
resource, which involves the directionality of the resource and the payloads flowing through it.

Directionality here covers the fact that, for both sources and sinks, they may pull or push with
regards to their external resource. A source could "pull" data an external resource
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

Beyond direction, external resources also dictate the payload type. Handling payloads is another
tricky aspect of this design, as we need a generic way to specify the type of payload whether we're
handling it in a raw form, such as the bytes we might send for it over a TCP socket, or the internal
event form, such as if we wanted to generate a corresponding `Event` to pass to a transform. We also
have to consider this translation when dealing with external resources and wanting to validate what
the component received/sent: if we need to send a "raw" payload to a source, and then validate we
got the expected output event, that means needing to compare the raw payload to the resulting
`Event`.

As such, components will define their expected payload via a combination of an all-encompassing enum
of supported codecs, as well as the existing `DataType` enum. This will allow components to describe
not only how the event should be encoded/decoded, but also what the actual contents should be, as
not all components support all event types.

The aforementioned enum will cover all of our standard codecs -- and will support conversion trait
implementations to make it easy to convert back-and-forth -- as well as any custom/specific codecs
that need to be supported. For every combination of codec and event data type, we'll supply a mock
event that has a fixed structure, such that for any combination, we would be able to translate
between the "raw" form and the internal `Event` form. This provides the basis of being able to
consistently and repeatably compare both inputs/outputs if such a validation pass is desirable. It
also means that we can generate events based purely on their data type, and ensure that regardless
of the external resource, those events can be converted to the correct "raw" form, depending on the
configured codec.

#### Pluggable validators

With the ability to build an arbitrary component, as well as the ability to build any external
resource it depends on, we also need to define what validation of the component actually looks like.
This is where **validators** come into play.

Validators are simple implementations of a new trait, `Validator`, that are able to collect
information about the component before and after the validation run is completed, and be transformed
into more human-friendly output to be shown at the conclusion of a component validation run, whether
the component is in compliance or discrpeancies were observed.

Let's take a brief look at `Validator` trait:

```rust
pub trait Validator {
    /// Gets the unique name of this validator.
    fn name(&self) -> &'static str;

    /// Runs the pre-hook logic for this validator.
    fn run_pre_hook(&mut self, component_type: ComponentType, inputs: &[Event]) {}

    /// Runs the post-hook logic for this validator.
    fn run_post_hook(&mut self, outputs: &[Event]) {}

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
  `Validator::run_post_hook` is run, getting a list of all output payloads that were collected
- finally, each validator is transformed into a `Result`-based human-friendly validation output via
  `Validator::into_results`, which will generally be nothing in the success/`Ok(...)` case, but will
  generally include human-friendly error messages in the failure/`Err(...)` case, such as which item
  did not pass validation, and so on

Validators are intended to hook into the environment to do most of their collecting, such as
checking all metrics emitted so far right before the component validation is run, and then seeing
what metrics were emitted during the test by checking again right after the component validation run
has completed. In the case of metrics, this could mean ensuring the global recorder is in
thread-local "test" mode, and clearing it before the run, and then after the run, collecting all
metrics that are present.

We provide the inputs and outputs but not all components will actually generate an output for each
input -- transforms being the simplest example of where there's not always a one-to-one mapping --
so this is primarily for basic checking or calculation of expected values, such as what the count of
"received events" for a component should be, and so on.

#### The component runner

Finally, with all of these primitives, we need a way to bring them all together, which is handled by
the component runner, or `Runner`. This type brings all of the pieces together -- the component to
validate, the validators to run -- and handles the boilerplate of building the component to validate
and any external resource, generating the inputs, sending the inputs, collecting the outputs, and
actually driving the component and external resource until all outputs have been collected.

Here's a simple example of what I expect this to look like:

```rust
fn get_all_validatable_components() -> Vec<&'static dyn Component> {
    // Collect all validatable components by using the existing component registration mechanism,
    // `inventory`/`inventory::submit!`, to iterate through sources, transforms, and sinks, collecting
    // them into a single vector.
}

#[test]
fn compliance() {
    crate::test_util::trace_init();

    let validatable_components = get_all_validatable_components();
    for validatable_component in validatable_components {
        let mut runner = Runner::from_component(validatable_component);
        runner.add_validator(StandardValidators::ComponentSpec);
        runner.add_validator(StandardValidators::MetricsConsistency);

        match runner.run_validation() {
            Ok(results) => {
              for validator_result in results.validator_results {
                match validator_result {
                  // Getting results in the success case will be rare, but perhaps we want to always print
                  // successful validations so that we can verify that specific components are being validated,
                  // and verify what things we're validating them against.
                  Ok(success_results) => {},
                  Err(failure_results) => {
                    let formatted_failures = failure_results.map(|s| format!(" - {}\n", s)).collect::<Vec<_>>();
                    panic!("Failed to validate component '{}':\n\n{}", validatable_component.name(), formatted_failures.join(""));
                  }
                }
              }
            },
            Err(e) => panic!("Failed to complete validation run for component '{}': {}", validatable_component.name(), e),
        }
        let results = runner.run_validation();
        assert!(results.is_ok());

        let validator_results = results.expect("results should be ok").validator_results;
        for validator_result in valid
    }
}
```

In this example, we can see that we create a new `Runner` by passing it something that implements
`Component`, which in this case we get from the existing `&'static` references to components that
have registered themselves statically via the existing `#[configurable_component(<type>("name"))]`
mechanism, which ultimately depends on `inventory` and `inventory::submit!`.

From there, we add the validators we care about. Similar to codecs, we'll have a standard set of
validators that should be used, so the above demonstrates a simple enum that has a variant for each
of these standard validators, and we use simple generic constraints and conversion trait
implementations to get the necessary `Box<dyn Validator>` that's stored in `Runner`. With validators
in place, we call `Runner::run_validation`, which spawns a current-thread Tokio runtime and does the
aforementioned building of components, external resources, input payloads, and drives all of those
pieces in the right order until the inputs have been fully processed and all potential outputs have
been collected. Our validator hooks are called at the right time, and once all outputs have been
collected, they're returned to the caller, which can then do normal error handling and formatting of
errors to signal that a component either passed validation or didn't, and if they didn't, why they
didn't.

We specifically spin up a current-thread Tokio runtime both to have a mechanism to drive asynchronous
code, but also to ensure that all generated events/metrics are emitted in the same thread where the
validators are running their pre/post hook logic. It also provides the invariant of "all components
run on the same thread" such that for any other subsystem that may need to be refactored/mocked for
the validators to capture necessary data, we can depend on it always being emitted on the same
thread, if that would make such a refactoring/mocked implementation easier to achieve.

## Rationale

- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?

## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives

- What other approaches have been considered and why did you not choose them?
- How about not doing this at all?

## Outstanding Questions

- List any remaining questions.
- Use this to resolve ambiguity and collaborate with your team during the RFC process.
- *These must be resolved before the RFC can be merged.*

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.

## Future Improvements

- List any future improvements. Use this to keep your "plan of attack" scope small and project a sound design.
