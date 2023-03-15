# RFC 7694 - 2021-06-01 - Improving and Monitoring Build Performance of Vector

This RFC describes the need for improvement to not only the build performance of Vector, but also
monitoring around the performance such that we can maintain healthy performance in the long term.

## Scope

- Build performance from the developer perspective and CI perspective
- Utilization of our CI infrastructure
- [RFC 7027: Vector core extraction](https://github.com/vectordotdev/vector/issues/7027)
- [RFC 6531: Performance testing](https://github.com/vectordotdev/vector/issues/6531)

## Motivation

As time goes on, Vector has only gotten more expensive to build, both in terms of computational
resources and time spent. In order to provide a best-in-class observability hub, we need a
development cycle that affords us the ability to quickly test ideas and validate assumptions. When
we spend too much time building Vector, that cycle becomes longer, and we lose agility and
productivity.

## Internal Proposal

The plan of attack is multi-faceted, focusing both on improving the build performance, and laying
the groundwork for understanding the performance over time in the future.

### More granular compilation units

@blt is already tackling this through his continued work on [RFC
7027](https://github.com/vectordotdev/vector/issues/7027).  This RFC seeks to break up the project
structure of Vector such that changes in a far-off part of the codebase don’t force recompilation of
unrelated parts where possible.

This will speed up the development phase, as developers are often working in narrow areas of the
codebase.

### Cached compilation results

While the linker time typically dominates compiling Vector – due to its single-threaded nature – we
can extract value by caching the compilation of crates where possible.  Crate-level caching is
useful across the board: development, performance testing, and CI.

Using the existing [sccache](https://github.com/mozilla/sccache) project, we could begin caching
compilation results with relative ease.  This project wraps `rustc` directly, handling the logic of
caching compilation results and retrieving them when requested in the future.

As many of Vector’s dependencies don’t often change, developers could provide benefit to themselves
and others by “priming the pump” when they are the ones updating dependencies for chore PRs, and so
on.  Their initial compilation would then benefit those down the line.

Further, in the case of CI, systems like GitHub Actions are not contextually aware of the Rust build
system.  While they may be able to naively cache outputs that land on disk, they are not aware of
things like compiler flags, or compiler versions, and so could lead to confusing mismatch issues
that waste time and effort to debug.

### Use of a better linker

As mentioned above, much time is spent in the linker phase of building Vector, where the many
dependent crates that make up Vector are bundled together to produce a final executable.  For most
users building Rust projects today, they're using their system linker, which is single-threaded.
However, newer linkers exist that can exploit the parallelism of today's multi-core systems and
provide much faster link times.

We should explore the use of [lld](https://lld.llvm.org/), which is a linker from the LLVM project
itself, the compiler toolchain that underpins Rust itself. `lld` is optimized for multi-core
systems, and generally more performance-focused than system linkers like GNU's `gold` linker.  In
some cases, `lld` shows a 3-5x improvement over `gold`, which represents a potential link time
reduction on the order of _minutes_ when used for Vector.

While Rust is moving towards bundling and using LLD
[directly](https://github.com/rust-lang/rust/issues/39915), and by default, that work is not yet
complete.  However, it is trivial to manually specify using `lld` directly when building a Rust
project, and usage by developers building Rust projects already shows that it is fairly stable
and is able to successfully link many large projects.

### Rework “release” profile for non-versioned releases

We currently utilize the same Cargo “release” profile whether we’re minting a versioned release or
simply running a release build locally for performance testing.  While a boon for our users, our
usage of LTO and modified “codegen-units” represents a significant increase in compilation time.  On
a standard-issue laptop used by Vector engineers, it can take upwards of 40 minutes to build a
release binary with these settings enabled.  With the settings disabled, the same build takes around
27 minutes, or 35% less time.

These settings can be added back during the build process, when building versioned releases, without
much effort.

### Add system telemetry to GitHub Actions runners

As GitHub Actions natively provides no telemetry of any kind about runners, or even basic metrics
(how long did a job sit before running? etc), we’re often in the dark when it comes to CI and runner
performance as a whole.

We should be running the Datadog agent on every runner possible, collecting both high-level system
metrics as well as more fine-grained metrics, such as Docker metrics on a per-container basis.
These metrics will, I believe, prove invaluable to debugging issues with the overall CI pipeline and
its performance over time.  While understanding the build performance will likely come down to more
advanced tooling used directly by engineers, we can’t optimize things like “use all the cores
available” or “we’re using too much memory during CI runs” unless we actually have some data, any
data, from those runs.

### Manually instrument CI to report build performance

Once we were properly utilizing Datadog on our CI runners, we could then instrument CI builds
directly to report the build time to Datadog, allowing us to track the build performance over time.

This data would be sparse given how many builds are likely to be triggered on a given day, but would
form the start of tracking build performance holistically.

## Rationale

Currently, as mentioned above, it can take nearly 40 minutes for a clean release build of Vector.
Depending on the hardware used, this can be reduced to 25 - 30 minutes, but the time spent is
untenable even at the 25 - 30 minute range.  This build time not only permeates the local
development experience, but everything happening in CI, as well.  Any step of the lifecycle of a
change that involves Vector is unnecessarily slowed down.  The build time of Vector is a massive
force multiplier.

If we changed nothing about the process, we could still successfully work on Vector, albeit with a
slower rate of change.  Given the need to occasionally pivot to new features, or bug fixes, or
whatever is the most important task at hand, rebuilding Vector is actually more common than one
might expect, and thus every move we make -- whether it be working on a new feature, or fixing a bug
-- will be slowed down by suboptimal build performance.

## Prior Art

Existing prior art matches the path being proposed here.  There is no one solution to build
performance, as it is a function of many different settings working in concert.

## Drawbacks

There is a risk that the implementation of this proposal potentially results in occasional
drawbacks, namely two: `sccache` poisoned cache, and unobservable performance regressions.

As `sccache` caches compiled dependencies, there is the risk that a dependency could be compiled
such that it is misconfigured, or if it was compiled with bad settings, it would then be reused even
after the issue was fixed, leading to a confusing development experience.  Practically speaking, it
is simple to clear the cache, but it would be a new potential line item for things to check when
debugging an unclear/confusing compilation error, or functional bug.

As well, we currently share the same Cargo profile whether we're benchmarking locally or building a
customer-facing release of Vector.  There is the risk that performance issues which are not observed
locally using a build-time-friendly Cargo profile may occur when using the customer-facing-release
Cargo profile.  While we can ensure that our CI benchmarks are optimized in the exact same way as
customer-facing builds, we'll still need to make sure we build robust CI benchmarking that we can
depend on as the final go/no go for performance-critical changes.

## Alternatives

Build performance is fairly constrained to the areas mentioned in the proposal, so there does not
exist an immediately obvious alternative to improve build performance as such.

## Outstanding Questions

- Are there other avenues we aren't exploring here that could provide a similar or more substantial
  improvement to build performance?

## Plan Of Attack

- [ ] Update our self-hosted GitHub Actions runners to run the Datadog Agent, and begin collecting
  telemetry on their overall utilization throughout a normal work day
- [ ] Add a new CI build step which simply builds Vector in release mode, from a clean workspace,
  and reports the build time to Datadog for over-time tracking
- [ ] Execute existing performance tests between current "release" profile and "build-optimized
  release" profile to ensure they are within a reasonable margin of error
- [ ] Change our Cargo profiles to use the "build-optimized release" profile by default, and switch
  to the "customer-facing release" profile when building via CI
- [ ] Test `lld` in CI to observe the potential maximum speedup we can expect
- [ ] Create a repeatable process/docs for Vector developers to be able to utilize `lld` locally
- [ ] Test `sccache` in CI to observe the potential maximum speedup we can expect
- [ ] Create a repeatable process/docs for Vector developers to be able to utilize `sccache` locally
