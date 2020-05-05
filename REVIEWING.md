# Reviewing

Pull request reviews are required before merging code into Vector. This document
will outline Vector's pull request review requirements. The following checklist
should be used for all pull requests:

- [ ] Are you the code owner for the files that have changed? If not, please involve the appropriate code owner(s). (see [Code Owners](#code-owners))
- [ ] Is the code addressing a single purpose? If not, the pull request should be broken up. (see [Single Purpose](#single-purpose))
- [ ] Is the code readable and maintainable? If not, suggest ways to improve this. (see [Readability](#readability))
- [ ] Is the code reasonably tested? If not, tests should be improved. (see [Testing](#testing))
- [ ] Is code marked as unsafe? If so, verify that this is necessary. (see [Safe Code](#safe-code))
- [ ] Is backward compatibility broken? If so, can it be avoided or deprecated? (see [Backwards compatibility](#backwards-compatibility))
- [ ] Have dependencies changed? (see [Dependencies](#dependencies))
- [ ] Has the code been explicitly reviewed for security issues? Dependencies included. (see [Security](#security))
- [ ] Is there a risk of performance regressions? If so, have run the [Vector test harness](https://github.com/timberio/vector-test-harness)? (see [Performance Testing](#performance-testing))
- [ ] Should documentation be adjusted to reflect any of these changes? (see [Documentation](#documentation))

## Backward Compatibility

All changes should strive to retain backward compatibility. If a change breaks
backward compatibility, it is much less likely to be approved. It is highly
recommended you discuss this change with a Vector team member before investing
development time.

## Code Of Conduct

If you have not, please review Vector's [Code of Conduct](CODE_OF_CONDUCT.md)
to ensure reviews are welcoming, open, and respectful.

## Code Owners

Code owners should be involved in reviews that touch their code. This is
defined in the [`.github/CODEOWNERS`](.github/CODE_OWNERS) file. Involving code
owners ensures all context is applied to reviews since they have intimate
knowledge of the code.

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

## Performance Testing

Vector currently offers 2 methods for performance testing:

1. Internal benchmarks located in the [`/benches` folder](benches).
2. A full end-to-end [test harness][urls.vector_test_harness] for complex
   integration and performance testing.

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
security issues. See [Vector's Security guide for more info](SECURITY.md).

## Testing

Code should be reasonably tested. Vector does not require 100% test coverage.
We believe this level of coverage is unnecessary. As a general rule of thumb,
we strive for 80% coverage, beyond this returns are diminishing. Please use
your best judgment, some code requires more testing than others depending
on its importance.

[urls.vector_test_harness]: https://github.com/timberio/vector-test-harness/
