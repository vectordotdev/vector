# RFC 6531 - 2021-02-23 - Performance Testing

This RFC documents the technical details of our existing performance testing
process and describes avenues for future work to improve over the state of the
art in this project.

* [Summary](#summary)
* [Motivation](#motivation)
* [Motivating Examples](#motivating-examples)
  * [Regression from Increased Instrumentation](#regression-from-increased-instrumentation)
  * [musl libc Release Builds](#musl-libc-release-builds)
  * [`metrics` Crate Upgrade Regresses Benchmarks](#metrics-crate-upgrade-regresses-benchmarks)
  * [Channel Implementation Regresses Topology Throughput](#channel-implementation-regresses-topology-throughput)
  * [Lua Transform Leaked Memory](#lua-transform-leaked-memory)
* [State of the Art](#state-of-the-art)
  * [Criterion](#criterion)
  * [test-harness](#test-harness)
* [Alternatives for Future Work](#alternatives-for-future-work)
  * ["Do Nothing"](#do-nothing)
  * [Lean Further Into Criterion](#lean-further-into-criterion)
  * [Build a `vector diagnostic` Sub-Command](#build-a-vector-diagnostic-sub-command)
  * [Run test-harness Nightly](#run-test-harness-nightly)


## Summary

This RFC proposes that we continue to improve our criterion benchmarks, both in
terms mere code coverage and by reducing their time to report in results. As
[iai](https://github.com/bheisler/iai) matures we may consider transition our
wall-clock benchmarks to instruction count based methods, but this RFC makes no
comment on that other than to mention it as a future possibility. This RFC also
proposes that we continue our use of
[vector-test-harness](https://github.com/timberio/vector-test-harness/),
expanding that project to run sustained "nightly" performance stress tests for
representative workloads.

More fundamentally, this RFC proposes that we continue/adopt three high-level
processes in our work on Vector:

  1. Performance is a first-class testing concern for Vector. We will drive our
     process to identify regressions or opportunities for optimization as close
     to introduction as possible.
  1. Identifying _that_ a regression has happened is often easier than _why_. We
     will continuously improve Vector’s diagnosis tooling to reduce the time to
     debug and repair detected issues.
  1. Performance regressions will inevitably, unintentionally make their way
     into a release. When this happens we will treat this just like we would a
     correctness regression, relying on our diagnostic tools and rolling the
     experiences of repair back into the tooling.

Vector's primary optimization target is **throughput**. All other concerns being
equal, Vector will choose to optimize toward this goal. However, unlike with
correctness testing, other concerns are not always equal and we may decide to
intentionally regress performance to achieve other aims.

## Motivation

Vector is a high-performance observability data pipeline. Part of its utility
for our end users comes in maintaining and improving this performance even in
the face of our users’ distinct goals for Vector and deployments of. The "high
surface area" of Vector makes maintaining the integrity of its performance
characteristics challenging and we have to put in real effort to do so.

## Motivating Examples

### Regression from Increased Instrumentation

PR: https://github.com/timberio/vector/pull/4765

Tracing instrumentation was added to `LogEvent` at all log levels, incurring
trace overhead even when trace logs were flagged off. This change made it out
into the world before being detected.

### musl libc Release Builds

PR: https://github.com/timberio/vector/issues/2030

Introduction of [musl libc](https://musl.libc.org/) regressed performance on
tcp-to-tcp benchmark from 0.7.0 to 0.8.0 in double digit percentages. This was
caught by the [test-harness](https://github.com/timberio/vector-test-harness/)
as the PR documents but only after the releases were cut since test-harness was
not running regularly.

### `metrics` Crate Upgrade Regresses Benchmarks

Issue: https://github.com/timberio/vector/issues/6412
PR: https://github.com/timberio/vector/pull/6217#issuecomment-766435360

Upgrading the `metrics` crate to `v0.13.1` regresses our criterion benchmarks by
20%. Our criterion benchmarks acted as a stop on this change making it into a
release but do not necessarily guide the work on repairing the issue.

### Channel Implementation Regresses Topology Throughput

Issue: https://github.com/timberio/vector/issues/6043

PR [5868](https://github.com/timberio/vector/pull/5868) introduced a change that
modified the buffer internals to use tokio-0.2's channel implementation. This
regressed test-harness benchmarks and showed mixed results in criterion
benches. For especially sensitive areas of the project -- like the topology --
we will absolutely have to rely on a battery of complementary approaches.

### Lua Transform Leaked Memory

Issue: https://github.com/timberio/vector/issues/1496
PR: https://github.com/timberio/vector/pull/1990

User reports that 0.6.0 steadily consumes memory resources in their deployment,
indicating a classic leak pattern in their monitoring. Once user provided their
configuration it became clear that the lua transform was not properly GC'ing, a
quirk of how lua defers GC runs.

## State of the Art

As of this writing there are two broad approaches for performance testing work
in the Vector project. They are:

  * criterion benchmarks
  * [vector-test-harness](https://github.com/timberio/vector-test-harness/)

### Criterion

The [criterion](https://github.com/bheisler/criterion.rs) benchmark library is a
Rust adaptation of the Haskell library of the same name. It runs specially
prepared tests repeatedly, timing each run compared to wall clock and building
up a statistical profile of the times to say with some (configurable) certainty
how much time the tested code takes to run for the given inputs. The benchmark
is only as representative as the test is of the final program's over capability
and especially short executions will fluctuate wildly. Because criterion runs
the test repeatedly up to some total time interval the runtime of a criterion
test can be quite long. Despite best efforts criterion tests suffer the same
difficulties with regard to fluctuation that plague other wall-clock test
methods, especially in noisy CI machines. See [this
article](https://pythonspeed.com/articles/consistent-benchmarking-in-ci/) by
Itamar Turner-Trauring,
[this](https://buttondown.email/nelhage/archive/f6e8eddc-b96c-4e66-a648-006f9ebb6678)
by Nelson Elhage and [this](http://www.serpentine.com/criterion/tutorial.html)
by Bryan O'Sullivan on the challenges of reliable benchmarking. Notably,
O'Sullivan's criterion does a fair bit more math than Rust's criterion to make
the results stable, so there is some area for improvement in Rust criterion but
the fundamental problem remains. It is possible that criterion will gradually
incorporate instruction count benchmarking, a reasonable proxy for performance,
but that is very early days yet.

We can see from [Motivating Examples](#motivating-examples) that this benchmark
work is paying off and has acted as a backstop, disallowing serious regressions
from making it into releases. A good deal of work has been done to make results
stable in our Github Actions based CI. PR build times are, however, increasing
at a steady clip, a drag on productivity and potentially a barrier to
open-source contributors whose engagement with a change may not be as high as
full-time employees on the Vector project.

We are actively expanding our use of criterion in the project, as of this
writing.

### test-harness

The [vector-test-harness](https://github.com/timberio/vector-test-harness/) is a
"black-box" performance and correctness testing approach. Performance tests
serve two roles: indicating whether Vector has suffered regressions for given
workloads and comparing Vector to competitor products. The later role feeds our
product documentation.  Let's consider the [disk
buffer](https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_performance)
performance test. This test is meant to probe the performance characteristic of
the "disk buffer", the disk backed variant of Vector's
[`buffers`](https://github.com/timberio/vector/blob/2ac861e09f99036145749ee8af7a7e0d7aa945c6/src/buffers/mod.rs). The
[README](https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_performance)
for the test describes the high-level approach: generate as much data as
possible in 60 seconds and observe the results of the test on average IO
throughput, CPU consumption and so forth. The test-harness uses
[ansible](https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_performance/ansible)
to set up and execute every variant to be tested. System operation during the
execution is recorded with [dstat](http://dag.wiee.rs/home-made/dstat/) and only
one iteration of each test is made. Tests are made on AWS spot instance c5.large
variant. The observations made by dstat are shipped to s3 for long-term storage.

The relatively short duration of this test and its singular instance will tend
to make the results quite noisy. Spot instances suffer no performance penalty
compared to reserved capacity but the c5.large may not be representative of the
machines Vector is deployed to. Only a single build of Vector is run here as
well. Consider that the
[0.11.1](https://github.com/timberio/vector/releases/tag/v0.11.1) supports
aarch64, armv7, x86_64, amd64, arm64, armhf, etc plus a cross product on some of
these platforms with different libc implementations. The test-harness executes
on Ubuntu, meaning tests are limited to Debian packaged Vector releases. The
data that dstat collects is relatively black box, especially in comparison to
tools like [perf](https://perf.wiki.kernel.org/index.php/Main_Page) or custom
eBPF traces, nor does the harness run regularly.

We can see from [Motivating Examples](#motivating-examples) that the
test-harness work has paid dividends. Irregular runs, multiple test purposes --
correctness, performance, competitor comparison -- noisey results and relatively
coarse information collected from the performance tests are all areas for
improvement. As an example, because of the short run duration the lua

We are not actively expanding the use of test-harness as of this writing, though
it is maintained and still runs.

## Alternatives for Future Work

In this section we will describe alternatives for future work with regard to our
performance testing in the Vector project. A later section will argue for a
specific alternative.

### "Do Nothing"

In this alterative we make no substantial changes to our practices. We will
continue to invest time in expanding our criterion benchmarks and will
periodically run the test-harness, expand it as seems desirable. We will also
not expand on Vector's self-diagnostic tools, except as would happen in the
normal course of engineering work.

#### Upsides

  * We continue to reap the benefits of our criterion work.
  * We do not have to substantially change our approach to performance work.

#### Downsides

  * Without improving the reliability of test-harness data we will continue to
    find its results difficult to act on.
  * If we do not improve Vector's self-diagnostic capbility we will struggle to
    understand user's on-prem issues, currently a very high-touch process. As our
    user base expands this problem will become more accute.
  * As our criterion tests increase in coverage the build time will balloon. This
    will steadily drain our productivity as iteration loop time increases.

### Lean Further Into Criterion

In this alternative we make no substantial changes to the test-harness -- follow
"do nothing" here -- but place more emphasis on the criterion work. In
particular, we intend:

  * to increase the amount of compute available to the criterion CI, throwing
    more hardware at the CI time issue,
  * to build benchmarks that demonstrate key components' throughput performance,
    ensuring that these numbers are maintained in documentation for end users,
  * to substantially improve the coverage of benchmarked Vector code, though as
    with correctness tests exact thresholds are a matter for team debate,
  * to run our criterion benchmarks across all supported Vector platforms and
  * to explore alternatives to improve criterion to derive better, more stable
    signals on from our benchmarks.

#### Upsides

  * We reap the benefits of broader adoption of criterion in our project, which
    include catching some regressions, offering targetted feedback to engineers
    (if a test is, itself, targetted) and improve the broader ecosystem by
    rolling changes into criterion.
  * We gain a good deal of detailed insight into Vector at a unit level.
  * We gain documented performance expectations for major components, a boon for
    our end users when evaluating Vector.

#### Downsides

  * As we have seen in practice, micro-benchmarks may not be representative of
    macro-performance.
  * Wall-clock benchmarks are extremely sensitive to external factors.
  * We will encounter situations where benchmarks improve in CI and regress on
    user's machines, especially as benchmarks become more "micro".

### Build a `vector diagnostic` sub-command

In this alternative we extend the Vector interface to include a `diagnostic`
sub-command. This diagnostic will examine the system to gather information about
it's running environment and perform, time fundamental actions. Information we
might want to collect:

  * What operating system is Vector running on?
  * For the directories present in Vector's config, what filesystems are in use?
    What mount options are in use?
  * How many CPUs are available to Vector and of what kind? How much memory?
    NUMA?
  * What kernel parameters are set, especially those relating to common
    bottlenecks like network, descriptor limits etc?
  * How long does a malloc/dealloc take for a series of block sizes?
  * How long does spawning a thread take?
  * How long do 2, 4, 8, 16 threads take to lock and unlock a common mutex?
  * For filesystems where Vector has R/W privileges, what IO characteristics
    exist for these filesystems?

This list is not exhaustive and hopefully you get the sense that the goal is to
collect baseline information about the system to inform user issues and guide
engineering work. An additional `doctor` sub-command could use the diagnostic
feature to examine a config and make suggestions or point out easily detected
issues, a disk buffer being configured to use a read-only filesystem, say.

#### Upsides

  * We collect system information from users on a case by case basis. This
    automates some of that information gathering.
  * Diagnostics information will help us discover unusual user systems that we
    might otherwise struggle to reproduce.

#### Downsides

  * The benefits of a `diagnostic` sub-command are not immediate, are focused on
    the after-release side of regression and some users will not wish to share
    its output with us.
  * A `diagnostic` sub-command is not a solution in itself but must be paired
    with other approaches.

###
