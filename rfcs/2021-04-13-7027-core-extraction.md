# RFC 7027 - 2021-04-13 - Core Extraction

This RFC describes the technical details of vector's "core", why we believe it
should be extracted as a distinct crate from "non-core" components and the
process we'll take to do said extraction.

* [Summary](#summary)
* [Motivation](#motivation)
* [The Structure of the Project](#the-structure-of-the-project)
* [Alternatives for Future Work](#alternatives-for-future-work)
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
mean intregrating with more sources of data, allowing our users to do new things
with their in-flight data and also consistently, release after release, use less
resources to do it. As noted in the summary build times are long and growing,
imposing a burden on development. It is _hard_ to benchmark and improve your
code if the feedback cycle is 30 minutes at least. Our flat structure
contributes raises the barrier to entry for contributions; users on discord have
referred to our compile times as "punishing". If a user contributes a new sink,
say, then they are responsible for:

* writing the sink and its correctness tests,
* writing benchmarks,
* resolving any incompatibilities between their new code and the existing vector
  model and
* waiting for CI to approve their changes.

With regard to benchmarks we generally do not request contributors to add them
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
[5374](https://github.com/timberio/vector/pull/5374) -- does suggest that
aggressive intregation of a cross-cutting concern will make things challenging.

We hope to capture the benefits to development we've seen in the VRL sub-package
and make them more broadly available in the vector project.

## The Structure of the Project

Today there are 168 top-level features in the project, as measured on commit
48d2a84b1b11ba54db7bd892944f2a479238edb4:

```
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
targets, some map -- not quite precisely -- to source/transform/sinks being
enabled. I had hoped to provide a dependency graph of these features but their
number and relation meant that what was generated was not explicable. Some
features seem to do almost but not exactly quite the same things.

Taken in the abstract the core of vector is a data ingest, transformation and
egress framework, expressed as an acyclic graph with nodes being separated by a
queue -- both in-memory and on-disk -- with associated mechanisms durability
across restarts, acks, backpressure between nodes in the graph and
self-instrumentation. Nodes are of type "source", "sink" and "transform". A
"source" node crates new `Event` instances -- this is vector's internal data
type -- and a "sink" destroys them, possibly by egressing them but also possibly
by just deallocating. A "transform" node modifies `Event` instances as they pass
through, destroys them, merges them or creates additional `Event`
instances. Configuration and reload management of, implementations of
source/transform/sink for different domains, common mechanism for backoffy/retry
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

## Alternatives for Future Work

In this section we will describe the alternatives for future work we might take
with regard to this RFC. A later section will argue for specific
alternatives. Please keep in mind that our goals are to:

* reduce iteration costs in engineering work,
* reduce the burden for casual contributors,
* enable more focused correctness and performance tests and
* reduce experimentation costs, pushing toward a fail-fast model of work.

### "Do Nothing"

In this alter

## Proposal

## Plan of Action

We have, in Rust, the following concepts to use in the organization of a
project:

* a "module" which contains functions, types etc
* a "crate" which contains one or more modules
* a "package" which contains one or crates
* a "workspace" that contains one or more packages

The vector project

with no
The core of vector is a data pipeline, expressed as a acyclic graph with nodes
being separated by queues. between with the following feature set:

* in-memory and on-disk paging . It , mostly in-memory with acks,
retry/backoff functionality, durability across restarts and hooks for extension.


Options:

* leave things as they are
* tidy up features, leave structure in place
* everything in packages, top-level crate is where features live
* top-level crate is core, packages are flagged on by features
* top-level crate combines packages -- one of which is core -- and "default"
  feature is untouched


TODO
