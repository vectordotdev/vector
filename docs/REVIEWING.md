# Reviewing

- [Checklist](#checklist)
- [Expectations](#expectations)
- [Backward Compatibility](#backward-compatibility)
- [Code Of Conduct](#code-of-conduct)
- [Dependencies](#dependencies)
- [Documentation](#documentation)
- [Performance Testing](#performance-testing)
- [Single Concern](#single-concern)
- [Readability](#readability)
- [Safe Code](#safe-code)
- [Security](#security)
- [Testing](#testing)

## Checklist

Pull request reviews are required before merging code into Vector. This document
will outline Vector's pull request review requirements. The following checklist
should be used for all pull requests:

- [ ] Is the code addressing a single purpose? If not, the pull request should be broken up. (see [Single Concern](#single-concern))
- [ ] Is the code readable and maintainable? If not, suggest ways to improve this. (see [Readability](#readability))
- [ ] Is the code reasonably tested? If not, tests should be improved. (see [Testing](#testing))
- [ ] Is code marked as unsafe? If so, verify that this is necessary. (see [Safe Code](#safe-code))
- [ ] Is backward compatibility broken? If so, can it be avoided or deprecated? (see [Backward compatibility](#backward-compatibility))
- [ ] Have dependencies changed? (see [Dependencies](#dependencies))
- [ ] Has the code been explicitly reviewed for security issues? Dependencies included. (see [Security](#security))
- [ ] Is there a risk of performance regressions? If so, have run the [Vector test harness](https://github.com/vectordotdev/vector-test-harness)? (see [Performance Testing](#performance-testing))
- [ ] Should documentation be adjusted to reflect any of these changes? (see [Documentation](#documentation))

For component changes, especially pull requests introducing new components, the
following items should also be checked:

- [ ] Does it comply with the [configuration spec](specs/configuration.md)?
- [ ] Does it comply with [component spec](specs/component.md)?
- [ ] Does it comply with the [instrumentation spec](specs/instrumentation.md)?

### Checklist - new source

This checklist is specific for Vector's sources.

- [ ] Does the source handle metrics? If it does, the Datadog Origin Metadata function (`sinks::datadog::metrics::encoder::source_type_to_service`),
      which maps the source to the correct Service value, needs to be updated. If this source is an Agent role and thus is the true origin of it's
      metrics, this will need to be a follow-up PR by a member of the Vector team.

### Checklist - new sink

This checklist is specific for Vector's sinks.

#### Logic

- [ ] Does it work? Do you understand what it is supposed to be doing?
- [ ] Does the retry logic make sense?
- [ ] Are the tests testing that the sink is emitting the correct metrics?
- [ ] Are there integration tests?

#### Code structure

- [ ] Is it using the sink prelude (`use crate::sinks::prelude::*`)?
- [ ] Is the sink a stream based sink?
      Check that the return value from `SinkConfig::build` is the return from `VectorSink::from_event_streamsink`.
- [ ] Is it gated by sensible feature flags?
- [ ] Is the code modularized into `mod.rs`, `config.rs`, `sink.rs`,  `request_builder.rs`, `service.rs`
- [ ] Does the code follow our [style guidelines].

#### Documentation

- [ ] Look at the doc preview on Netlify. Does it look good?
- [ ] Is there a `cue` file linking to `base`?
- [ ] Is there a markdown file under `/website/content/en/docs/reference/configuration/sinks/`?
- [ ] Are module comments included in `mod.rs` linking to any relevant areas in the external services documentation?

#### Configuration

- [ ] Are TLS settings configurable?
- [ ] Are the Request settings configurable?
- [ ] Should it have proxy settings? If so, are they in place?
- [ ] Does it need batch settings? If so, are they used?


## Expectations

We endeavour to review all PRs within 2 working days (Monday to Friday) of submission.

## Backward Compatibility

All changes should strive to retain backward compatibility. If a change breaks
backward compatibility, it is much less likely to be approved. It is highly
recommended you discuss this change with a Vector team member before investing
development time.

Any deprecations should follow our [deprecation policy](DEPRECATION.md).

## Code Of Conduct

If you have not, please review Vector's [Code of Conduct](../CODE_OF_CONDUCT.md)
to ensure reviews are welcoming, open, and respectful.

## Dependencies

Dependencies should be _carefully_ selected. Before adding a dependency, we
should ask the following questions:

1. Is the dependency worth the cost?
2. Is the dependency actively and professionally maintained?
3. Is the dependency experimental or in the development phase?
4. How large is the community?
5. Does this dependency have a history of security vulnerabilities?
6. Will this affect the portability of Vector?
7. Does the dependency have a compatible license?

## Documentation

Documentation is incredibly important to Vector; it is a feature and
differentiator for Vector. Pull requests should not be merged without adequate
documentation, nor should they be merged with "TODOs" opened for documentation.

Ideally all modules should have module level documentation. Module level
documentation can be omitted for modules where the purpose is obvious and covered
by a general pattern. With the sinks there is typically a standard number of modules
included (config.rs, request_builder.rs, service.rs, sink.rs, tests.rs),
these modules don't need documentation as they will be covered with higher level
documentation.

All `pub` and `pub(crate)` functions, structs and macros must have documentation.

Consider including examples for modules, structs, functions or macros that
will be well used throughout Vector.

See the [rustdoc](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html)
book for more details on writing documentation.

## Performance Testing

Vector currently offers 2 methods for performance testing:

1. Internal benchmarks located in the [`/benches` folder](../benches).
2. A full end-to-end [soak test
   suite](https://github.com/vectordotdev/vector/tree/master/soaks) for complex
   integration and performance testing.

For new integrations, consider whether a new soak test should be added.

## Single Concern

Changes in a pull request should address a single concern. This promotes quality
reviews through focus. If a pull request addresses multiple concerns, it should
be closed and followed up with multiple pull requests addresses each concern
separately. If you are unsure about your change, please open an issue and the
Vector maintainers will help guide you through the scope of the change.

## Readability

Code is read more than it is written. Code must be documented and readable.

## Safe Code

Unsafe code should be reviewed carefully and avoided if possible. If code is
marked as `unsafe`, a detailed comment should be added explaining why.

## Security

Security is incredibly important to Vector. Users rely on Vector ship
mission-critical and sensitive data. Please review the code explicitly for
security issues. See [Vector's Security guide for more info](../SECURITY.md).

## Testing

Code should be reasonably tested. Vector does not require 100% test coverage.
We believe this level of coverage is unnecessary. As a general rule of thumb,
we strive for 80% coverage, beyond this returns are diminishing. Please use
your best judgment, some code requires more testing than others depending
on its importance.

For integrations, consider whether the code could be integration tested.

[style guidelines]: https://github.com/vectordotdev/vector/blob/master/STYLE.md
