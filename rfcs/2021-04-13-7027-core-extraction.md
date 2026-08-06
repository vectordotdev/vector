# RFC 7027 - 2021-04-13 - Core Extraction

This RFC describes the technical details of vector's "core", why we believe it
should be extracted as a distinct notion from "non-core" components and the
process we'll take to do said extraction.

* [Summary](#summary)
* [Motivation](#motivation)
* [The Structure of the Project](#the-structure-of-the-project)
* [What is "core" to vector?](#what-is-core-to-vector)
* [Alternatives for Future Work](#alternatives-for-future-work)
  * ["Do Nothing"](#do-nothing)
  * [Reduce Features, Leave Structure](#reduce-features-leave-structure)
    * [Default Features is Empty](#default-features-is-empty)
  * [Top-Level is Core, Concepts Become Packages](#top-level-is-core-concepts-become-packages)
  * [Core is Just Another Package](#core-is-just-another-package)
* [Proposal](#proposal)
* [Plan of Action](#plan-of-action)

## Summary

Today on my host a full vector release build takes 30+ minutes. A debug build is
not much faster. A check build is a matter of minutes. These delays impose a
serious burden on vector development. Further, the vector code base today is
relatively _flat_, a boon in our early days when we weren't totally sure about
the organization of the project but today leading to very wide PRs and, worse,
very broad in-development work. This is worse because it means any change we
want to validate -- say, swapping `String` with `Box<str>` in key types --
requires a goodly chunk of the code base to be adjusted. This is fine when work
is demonstrated to have a positive effect but is wasted time when the change is
neutral or negative. As well, if a user desires a vector build with debug
symbols present this imposes a severe size constraint on the user _unless_ they
carefully curate among our current zoo of feature flags, or other custom build
goals. It is our belief that by extracting a vector "core" we can:

* reduce iteration costs in engineering work,
* reduce the burden for casual contributors,
* enable more focused correctness and performance tests and
* reduce experimentation costs, pushing toward a fail-fast model of work.

## Motivation

We want to push the vector project into new, more competitive spaces. This will
mean integrating with more sources of data, allowing our users to do new things
with their in-flight data and also consistently, release after release, use less
resources to do it. As noted in the summary, build times are long and growing,
imposing a burden on development. It is _hard_ to benchmark and improve your
code if the feedback cycle is 30 minutes. Our flat structure raises the barrier
to entry for contributions; users on discord have referred to our compile times
as "punishing". If a user contributes a new sink, say, then they are responsible
for:

* writing the sink and its correctness tests,
* writing benchmarks,
* resolving any incompatibilities between their new code and the existing vector
  model, and
* waiting for CI to approve their changes.

With regard to benchmarks, we generally do not request contributors to add them
as doing so is too complicated. Our flat structure promotes a conflation between
unit and integrated benchmark tests, not to mention you need a fair bit of
memory to _link_ vector to run these tests yourself. The feedback loop here is
hours long. Any incompatibilities between new and existing code are quite
time-consuming to resolve as our flat structure encourages a "big bang" mindset
to changes, if not mandates it. The flatness of our project structure does mean,
however, that relatively simple sources/transforms/sinks are convenient to
introduce, there is "one place" for such things.

The vector project does have some sub-packages. VRL -- in `lib/vrl` -- is one
such. The development experience here has a tighter feedback loop than in vector
proper. VRL benchmarks can be built independent of vector -- avoiding its link
cost -- and its tests end up clearly focused on VRL itself. This _does_ mean
that integration tests remain to be written in the top-level of the project but
the overall experience is improved. Changes in VRL are isolated to VRL, though
experience with `Lookup`/`LookupBuf` -- see
[5374](https://github.com/vectordotdev/vector/pull/5374) -- does suggest that
aggressive integration of a cross-cutting concern will make things challenging.

We hope to capture the benefits to development we've seen in the VRL sub-package
and make them more broadly available in the vector project.

## The Structure of the Project

Today there are 168 top-level features in the project, as measured on commit
48d2a84b1b11ba54db7bd892944f2a479238edb4:

```sh
> cargo read-manifest | jq ".features" | jq 'keys' | wc -l
[
  "all-integration-tests",
  "all-logs",
  "all-metrics",
  "api",
  "api-client",
  "aws-cloudwatch-logs-integration-tests",
  "aws-cloudwatch-metrics-integration-tests",
  "aws-ec2-metadata-integration-tests",
  "aws-ecs-metrics-integration-tests",
  "aws-integration-tests",
  "aws-kinesis-firehose-integration-tests",
  "aws-kinesis-streams-integration-tests",
  "aws-s3-integration-tests",
  "aws-sqs-integration-tests",
  "benches",
  "cli-tests",
  "clickhouse-integration-tests",
  "default",
  "default-cmake",
  "default-msvc",
  "default-musl",
  "default-no-api-client",
  "default-no-vrl-cli",
  "disable-resolv-conf",
  "docker-logs-integration-tests",
  "es-integration-tests",
  "gcp-cloud-storage-integration-tests",
  "gcp-integration-tests",
  "gcp-pubsub-integration-tests",
  "humio-integration-tests",
  "influxdb-integration-tests",
  "kafka-integration-tests",
  "kubernetes",
<SNIP>
```

The features are split in their purposes. Some flag on tests, some are for build
targets, some map to source/transform/sinks being enabled. I had hoped to
provide a dependency graph of these features but their number and relation meant
that what was generated was not explicable. Some features seem to do almost but
not exactly quite the same things.

Taken in the abstract the core of vector is a data ingest, transformation and
egress framework, expressed as an acyclic graph with nodes being separated by a
queue -- both in-memory and on-disk -- with associated mechanisms providing
durability across restarts, acks, back-pressure between nodes in the graph and
self-instrumentation. Nodes are of type "source", "sink" and "transform". A
"source" node creates new `Event` instances -- this is vector's internal data
type -- and a "sink" destroys them, possibly by egressing them but also possibly
by just deallocating. A "transform" node modifies `Event` instances as they pass
through, destroys them, merges them or creates additional `Event`
instances. Configuration and reload management of, implementations of
source/transform/sink for different domains, common mechanism for backoff/retry
in sinks and cross-cutting concerns like tracing integration are all non-core.

Our project today has a small number of packages in `lib`, notably:

* `lib/shared` -- a collection of common-ish types and functions, vector's "util"
* `lib/file-source` -- the backing code for the file source, minus some of, but
  not all of, the hook-up to be a proper vector source
* `lib/vrl` -- the implementation of VRL in its entirety

VRL is the gold standard in vector as of this writing. It's a package containing
several sub-crates with a gentle dependency graph. From the point of view of
vector there is a single interface to cope with but VRL developers have
_additional_, unexposed interfaces to aid their work and experimentation.

The top-level package in vector is where most of the code lives. This code is
relied on by its sub-packages and, so, contains pretty well every major concern
of vector, all sources, sinks and transforms. There are some self-contained
modules here -- `src/metrics` for example -- but someone working on these
modules will have to participate in full vector build times to test their
changes. Previous discussions concerning core extraction have noted that there
are clear division lines in the top-level. "All AWS related code could live in a
package, under a feature flag," which is true. All AWS related code could live
in the top-level crate but under a feature flag, equally well.

Our present feature flag crowd makes our testing surface high, though our CI
tooling has done a good job of catching any features I have forgot to enable in
addition to those present in "default" feature when I make changes. Tools like
[cargo-hack](https://github.com/taiki-e/cargo-hack) or
[cargo-all-features](https://lib.rs/crates/cargo-all-features) will help as
well.

## What is "core" to vector?

I argue that the following are "core":

* the `Event` type,
* the `Value` type,
* the `Pipeline` type,
* the `VectorSink` type,
* the `Source` type,
* the `Transform` type,
* the `topology` module,
* the `metrics` module,
* the `buffers` module and possibly
* the `adaptive_concurrency` module.

I do not believe this list is final and I recognize that some items here draw in
other unlisted types, like the `topology` module draws in `DisabledTrigger`. My
understanding of what is core versus not depends on whether the lack of a thing
would make vector other than what it is in its absence or leave us blind to
vector's runtime behavior. The loss of VRL makes vector significantly less
useful to our users but the loss of `Transform` makes vector something else
entirely. The core of vector should, as well, be a package of useful pieces for
_building_ a vector, not vector but with a bunch of pieces ripped out.

## Alternatives for Future Work

In this section we will describe the alternatives for future work we might take
with regard to this RFC. A later section will argue for specific
alternatives. Please keep in mind that our goals are to:

* reduce iteration costs in engineering work,
* reduce the burden for casual contributors,
* enable more focused correctness and performance tests and
* reduce experimentation costs, pushing toward a fail-fast model of work.

### "Do Nothing"

In this alternative we change nothing about the structure of our project and our
approach to it. This does not address any of our goals in this RFC.

### Reduce Features, Leave Structure

This alternative reduces the overall features in the project -- say, introducing
an "aws" flag that flips on all AWS related code -- but does not change its
overall structure. This would reduce the total number of features in the project
but maybe not as many as we would hope. For instance, if optimizing total
features is a goal do we excise flags like `transforms-add_fields`? If all
transforms were put behind a singular "transforms" flag this would optimize
feature totals but would pessimize current workflows that selectively build only
the transform being worked. The "core" is here extracted as the code that
remains when all feature flags are flipped off, though this will not be apparent
in the structure of the code necessarily.

This alternative does reduce iteration costs, potentially, but does not reduce
the burden for casual development, does not lead to more focused testing,
nor does it reduce
overall experimentation costs. For instance, an engineer working on a single
transform would still be responsible for linking all of vector if they were in a
benchmark/optimize loop.

#### Default Features is Empty

As a sub-alternative to "Reduce Features" I will note that today we often
encourage our engineers to compile with `--no-default-features` flagged on. This
_does_ reduce build times and can, depending on the area of the code base you're
working on, improve iteration loop speed. We might make this the official
default. This alternative would require us to modify our "release" build to flip
on the specific features we intend to ship. However, while this alternative does
potentially reduce iteration costs and experimentation costs depending on the
area being worked on, it does nothing for reducing the burden for casual
contributors, nor does it lead to more focused tests. A non-representative
default may be surprising to casual contributors, leading to CI dings that would
otherwise have happened locally.

### Top-Level is Core, Concepts Become Packages

This alternative pushes as much as possible into packages, leaving whatever
remains as the "core" of vector. All transforms would go into a "transforms"
package with sub-packages for large conceptual areas, similar for sinks and
sources. VRL would remain as-is and we'd need to debate in the future what is
and isn't "core". Once done this alternative would reduce iteration costs on
core and, if documented, give casual contributors guideposts about where to add
code. Experimentation costs could also be reduced, with packages in need of
modification because of changes to core being dealt with only after the changes
to core as shown to be worthwhile. This alternative does not make for a more
focused testing environment. With "core" as the top-level package we can't
follow the sub-package model of exposing internal interfaces for experimentation
as easily: our `vector` binary lives at the top-level, what if we want to create
a `core` binary at the top-level that takes no configuration and just sends data
through itself for testing purposes? Is that confusing to our users? I, at
least, would find it so. Should configuration be a "core" issue or a package
used exclusively by "core"? How granular should a package be? Must all sources
go in a "sources" package or can `file-source` continue, or should it be a crate
in a package? These are important questions and they'll need answers ahead of
time in this alternative. That is, "core" is extracted by bulk movement of code
in this approach.

### Core is Just Another Package

This alternative is the inverse of the previous: core is just another
package. We extract what code is "core" to vector and move it into its own
package, rigged with private tests, interfaces in the manner of VRL. This will
allow work on core to happen in a similar manner to VRL today, with tighter
iteration and more focus. Experimentation in core specifically need not include
the rest of the project, until after the experiment is shown to be
positive. Casual contributors will be unlikely to work with core -- most outside
contributions have historically been sources or sinks -- but, if they do, we'll
have a less resource intensive package for them to work against. Core as just
another package lends itself to an iterative approach: extraction of core pieces
need not happen in one burst but bit by bit. This approach has the added benefit
of deferring questions of non-core project structure, how our feature flags are
set up etc. There will be some grey area of course -- I argue that configuration
is not a core concern but am sympathetic to the counter -- but these can be
resolved in the piecemeal process of moving things into "core".

## Proposal

I argue that "Core is Just Another Package" is the best path forward. This
approach addresses each of our major goals for extracting core without imposing
significant secondary questions about total project organization, of which there
has been nervous concern in the team. We don't want to "cut the project
boundaries wrong" and I share this concern. I think we will eventually want more
rigorous structure in the non-core parts of the project but I would like to see
that deferred.

## Plan of Action

To extract the core of vector I propose that we do the following:

* Create a new, blank `core` package that top*level vector depends on.
* Migrate one of the areas from 'What is "core" to vector?' into this package,
  along with any existing test code.
* Add any new test code that seems worthwhile for the migrated material.
* Repeat.
* Once a critical mass of core exists, create a core-private source for
  generating `Event` load into a topology and a core-private sink that signals
  for program termination after a set number of `Event`s have been
  received. Bundle this into a `vector-core` for throughput trials of core only,
  validating the package structure for cheap experimentation and for avoiding
  core as "vector but with pieces missing".
* Migrate any remaining core concepts.

By the end of this process we will have a "core" package that represents all the
pieces needed to build the backbone of a vector, used in the top-level vector
package but independently able to be rigged into test harnesses and experimented
with in a manner that does not necessarily require chasing changes through the
project tree. We can then start the process of pushing core to have perfect
mechanical sympathy, in line with our goals in RFC 6531.
