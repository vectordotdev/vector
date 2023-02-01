# RFC 6531 - 2021-02-23 - Performance Testing

This RFC documents the technical details of our existing performance testing
process and describes avenues for future work to improve over the state of the
art in this project, then makes a proposal for where to go next.

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
  * [Rethink test-harness](#rethink-test-harness)
  * [Run Vector Continuously](#run-vector-continuously)
* [Proposal](#proposal)
* [Plan of Action](#plan-of-action)
  * [Ad Hoc Sprint](#ad-hoc-sprint)
  * [Criterion](#criterion)
  * [Test-Harness](#test-harness)
  * [Pursue `diagnostic` Command](#pursue-diagnostic-command)

## Summary

This RFC proposes that we continue to improve our criterion benchmarks, both in
terms of more code coverage and by reducing their time to report in results. As
[iai](https://github.com/bheisler/iai) matures we may consider transition our
wall-clock benchmarks to instruction count based methods, but this RFC makes no
comment on that other than to mention it as a future possibility. This RFC also
proposes that we continue our use of
[vector-test-harness](https://github.com/vectordotdev/vector-test-harness/),
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

PR: https://github.com/vectordotdev/vector/pull/4765

Tracing instrumentation was added to `LogEvent` at all log levels, incurring
trace overhead even when trace logs were flagged off. This change made it out
into the world before being detected.

### musl libc Release Builds

PR: https://github.com/vectordotdev/vector/issues/2030

Introduction of [musl libc](https://musl.libc.org/) regressed performance on
tcp-to-tcp benchmark from 0.7.0 to 0.8.0 in double digit percentages. This was
caught by the [test-harness](https://github.com/vectordotdev/vector-test-harness/)
as the PR documents but only after the releases were cut since test-harness was
not running regularly.

### `metrics` Crate Upgrade Regresses Benchmarks

Issue: https://github.com/vectordotdev/vector/issues/6412
PR: https://github.com/vectordotdev/vector/pull/6217#issuecomment-766435360

Upgrading the `metrics` crate to `v0.13.1` regresses our criterion benchmarks by
20%. Our criterion benchmarks acted as a stop on this change making it into a
release but do not necessarily guide the work on repairing the issue.

### Channel Implementation Regresses Topology Throughput

Issue: https://github.com/vectordotdev/vector/issues/6043

PR [5868](https://github.com/vectordotdev/vector/pull/5868) introduced a change that
modified the buffer internals to use tokio-0.2's channel implementation. This
regressed test-harness benchmarks and showed mixed results in criterion
benches. For especially sensitive areas of the project -- like the topology --
we will absolutely have to rely on a battery of complementary approaches.

### Lua Transform Leaked Memory

Issue: https://github.com/vectordotdev/vector/issues/1496
PR: https://github.com/vectordotdev/vector/pull/1990

User reports that 0.6.0 steadily consumes memory resources in their deployment,
indicating a classic leak pattern in their monitoring. Once user provided their
configuration it became clear that the lua transform was not properly GC'ing, a
quirk of how lua defers GC runs.

## State of the Art

As of this writing there are two broad approaches for performance testing work
in the Vector project. They are:

* criterion benchmarks
* [vector-test-harness](https://github.com/vectordotdev/vector-test-harness/)

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
stable in our GitHub Actions based CI. PR build times are, however, increasing
at a steady clip, a drag on productivity and potentially a barrier to
open-source contributors whose engagement with a change may not be as high as
full-time employees on the Vector project.

We are actively expanding our use of criterion in the project, as of this
writing.

### test-harness

The [vector-test-harness](https://github.com/vectordotdev/vector-test-harness/) is a
"black-box" performance and correctness testing approach. Performance tests
serve two roles: indicating whether Vector has suffered regressions for given
workloads and comparing Vector to competitor products. The later role feeds our
product documentation.  Let's consider the [disk
buffer](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/disk_buffer_performance)
performance test. This test is meant to probe the performance characteristic of
the "disk buffer", the disk backed variant of Vector's
[`buffers`](https://github.com/vectordotdev/vector/blob/2ac861e09f99036145749ee8af7a7e0d7aa945c6/src/buffers/mod.rs). The
[README](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/disk_buffer_performance)
for the test describes the high-level approach: generate as much data as
possible in 60 seconds and observe the results of the test on average IO
throughput, CPU consumption and so forth. The test-harness uses
[ansible](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/disk_buffer_performance/ansible)
to set up and execute every variant to be tested. System operation during the
execution is recorded with [dstat](http://dag.wiee.rs/home-made/dstat/) and only
one iteration of each test is made. Tests are made on AWS spot instance c5.large
variant. The observations made by dstat are shipped to s3 for long-term storage.

The relatively short duration of this test and its singular instance will tend
to make the results quite noisy. Spot instances suffer no performance penalty
compared to reserved capacity but the c5.large may not be representative of the
machines Vector is deployed to. Only a single build of Vector is run here as
well. Consider that the
[0.11.1](https://github.com/vectordotdev/vector/releases/tag/v0.11.1) supports
aarch64, armv7, x86_64, arm64, armhf, etc plus a cross product on some of
these platforms with different libc implementations. The test-harness executes
on Ubuntu, meaning tests are limited to Debian packaged Vector releases. The
data that dstat collects is relatively black box, especially in comparison to
tools like [perf](https://perf.wiki.kernel.org/index.php/Main_Page) or custom
eBPF traces, nor does the harness run regularly.

We can see from [Motivating Examples](#motivating-examples) that the
test-harness work has paid dividends. Irregular runs, multiple test purposes --
correctness, performance, competitor comparison -- noisy results and relatively
coarse information collected from the performance tests are all areas for
improvement. As an example, because of the short run duration the lua memory
leak described in [Lua Transform Leaked Memory](#lua-transform-leaked-memory)
had to be caught by a user.

We are not actively expanding the use of test-harness as of this writing, though
it is maintained and still runs.

## Alternatives for Future Work

In this section we will describe alternatives for future work with regard to our
performance testing in the Vector project. A later section will argue for a
specific alternative.

### "Do Nothing"

In this alternative we make no substantial changes to our practices. We will
continue to invest time in expanding our criterion benchmarks and will
periodically run the test-harness, expand it as seems desirable. We will also
not expand on Vector's self-diagnostic tools, except as would happen in the
normal course of engineering work.

Addresses:

* `metrics` Crate Upgrade Regresses Benchmarks

Does not address:

* Regression from Increased Instrumentation
* musl libc Release Builds
* Channel Implementation Regresses Topology Throughput
* Lua Transform Leaked Memory

#### Upsides

* We continue to reap the benefits of our criterion work.
* We do not have to substantially change our approach to performance work.

#### Downsides

* Without improving the reliability of test-harness data we will continue to
  find its results difficult to act on.
* If we do not improve Vector's self-diagnostic capability we will struggle to
  understand user's on-prem issues, currently a very high-touch process. As our
  user base expands this problem will become more acute.
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

Addresses:

* Regression from Increased Instrumentation
* `metrics` Crate Upgrade Regresses Benchmarks

Does not address:

* musl libc Release Builds
* Channel Implementation Regresses Topology Throughput
* Lua Transform Leaked Memory

#### Upsides

* We reap the benefits of broader adoption of criterion in our project, which
  include catching some regressions, offering targeted feedback to engineers
  (if a test is, itself, targeted) and improve the broader ecosystem by
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
it's running environment and perform time fundamental actions. Information we
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

Note that the work described here is also reflected in [Issue
4660](https://github.com/vectordotdev/vector/issues/4660). We should also consider
that the interface -- a sub-command -- is not the mechanism and leave open the
possibility for multiple interfaces to the same diagnostic information.

Does not address:

* Regression from Increased Instrumentation
* `metrics` Crate Upgrade Regresses Benchmarks
* musl libc Release Builds
* Channel Implementation Regresses Topology Throughput
* Lua Transform Leaked Memory

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

### Run test-harness Nightly

In this alternative we take the existing test-harness code base and adjust it to
run nightly (likely UTC 00:00 for convenience), building from the current head
of master branch. We will need to actually build a nightly Vector release for
use by test-harness, but this seems straightforward to achieve. We intend to
build this "nightly" process in such a way as to allow for arbitrary commits to
be run, though we do not intend to expose this behavior as a first step,
necessarily. Arbitrary commit execution will allow for ad hoc experimentation by
engineers, bisection of regressions.

Addresses:

* Regression from Increased Instrumentation
* musl libc Release Builds
* Channel Implementation Regresses Topology Throughput

Does not address:

* `metrics` Crate Upgrade Regresses Benchmarks
* Lua Transform Leaked Memory

#### Upsides

* By adjusting the test-harness to run nightly we reduce the total number of
  commits that can potentially introduce regressions, naturally simplifying the
  debugging process.
* We need to make very few changes to the test-harness to achieve this, other
  than rigging up a method for periodic execution.

#### Downside

* Other than reducing the number of commits that must be examined, this
  approach retains all the defects of the existing test-harness.

### Rethink test-harness

Consider that the test-harness currently serves three purposes: correctness
testing, performance regression testing and product comparison testing. The
first two have value with every commit, the third is valuable when we update
release documentation as a guide for new users and for keeping abreast of the
progress of our competition. In this alternative we:

* Make a logical, if not structural, split in the project between the
  different methods of testing.
* Exploit this split to performance tests in a "nightly" fashion, "comparison"
  and correctness tests for pre-releases or otherwise.
* Exploit this split to run only Vector in performance testing, ensuring that
  these tests are more straightforward to write and allowing us to write more
  of them as the cost to introduce each goes down.
* Run multiple instances of the same test and for significantly longer
  duration, applying statistical controls after the fact to get cleaner
  signal.
* Significantly expand the data collection we do from the subject host
  machine. We want to collect the same kinds of information that dstat does
  and then more, ideally with an aim to understanding the internal mechanism
  of Vector and how it plays against system resource constraints, in the [USE
  method](http://www.brendangregg.com/usemethod.html) sense.

Fully integrated correctness testing is beyond the scope of this RFC but implied
in this alternative is a subsequent conversation about our goals here.

Addresses:

* Regression from Increased Instrumentation
* musl libc Release Builds
* Channel Implementation Regresses Topology Throughput
* `metrics` Crate Upgrade Regresses Benchmarks

Does not address:

* Lua Transform Leaked Memory

#### Upsides

* By adjusting the test-harness to run nightly we reduce the total number of
  commits that can potentially introduce regressions, naturally simplifying the
  debugging process.
* Significantly expanded scope for performance testing of Vector.
* More stable results from the test-harness.
* Detailed insights about the internal mechanism of Vector in operation. The
  aim is to not just demonstrate a regression but point at _why_ a regression
  is.

#### Downsides

* Your author is only passingly familiar with operating systems outside Linux,
  especially for the kinds of tools we'd want to bring to bear here. Without
  additional experts we should consider that test-harness will continue to be
  Linux-only.
* test-harness will be a first class project in its own right, one that exists
  in an iterative cycle with Vector.

### Run Vector Continuously

In this alternative we run Vector ourselves in configurations that are
representative of our end users with conditions that are intended to be as
extreme as possible. We want to be our own, most difficult user. We will run
Vector in "bread and butter" setups, those that are likely to be in use by users
and push Vector above its thresholds. When performance improves we'll up the
pressure. Likewise, we will invest in continuous monitoring of Vector with
regard to key customer metrics and continuously monitor the correct operation of
Vector in its deployments. Vector should be run as close to production here as
possible, mimicking the kind of environment our users will run Vector in.

Addresses:

* Regression from Increased Instrumentation
* musl libc Release Builds
* Channel Implementation Regresses Topology Throughput
* `metrics` Crate Upgrade Regresses Benchmarks
* Lua Transform Leaked Memory

#### Upsides

* Gives significant confidence that Vector is fit for purpose for the
  configurations run.
* Discovers long-tail issues that other testing methods may not.

#### Downsides

* Detected issues may be very difficult track down to source in this
  environment. Vector will not be running in an instrumented environment,
  meaning we will have to rely on its self-diagnostic and self-telemetry
  capabilities.
* Detected issues may not be well-isolated in this environment. Long-tail
  problems are prone to collusion.
* Initial setup costs will need to be paid and we'll further have to consider
  how we introduce Vector changes into this system. Nightly? Pre-release?

## Proposal

This RFC proposes that we pursue the following work:

* [Lean Further Into Criterion](#lean-further-into-criterion)
* [Rethink test-harness](#rethink-test-harness)
* [Build a `vector diagnostic` Sub-Command](#build-a-vector-diagnostic-sub-command)

The first is a continuation of our existing criterion work, but more. With
enough compute power backing our CI process we can achieve fast feedback on
PRs. The results may be a touch artificial but fast feedback is key. The second
is not a hard break with our current test-harness work so much as it is a
re-framing of it and a refinement. We can iteratively work test-harness into a
more reliable, repeatable state without giving up any of the work done
there. The feedback cycle of test-harness is 24 hours at a minimum, meaning it
is complementary to the criterion work in this and in that it will, necessarily,
focus on all-up configurations.  Lastly the `diagnostic` sub-command helps us
with our existing user base and may be expanded as experience dictates. I
consider `diagnostic` a soft goal, where the first two are hard.

With regard to the remaining alternatives I do not believe that ["Do
Nothing"](#do-nothing) is acceptable for Vector. Our current state of the art is
an excellent foundation but we have already experienced its rough points, as
have our users. If we pursue "Rethink test-harness" I see [Run test-harness
Nightly](#run-test-harness-nightly) as a mere stepping stone in that greater
work, not an end state itself. Lastly, I adamantly believe that we will need to
pursue [Run Vector Continuously](#run-vector-continuously) in the future but the
feedback loop here is especially long. The approaches above will pay dividends
sooner and with more focus, where Run Vector Continuously is meant to search for
long-tail issues.

## Plan of Action

In this section we break down the plan of action for the proposed future work,
given in the previous section. There are three complementary, long-term efforts
here. They will be pursued in a roughly a concurrent manner, depending on how
many people are available for the work. As an example, the `diagnostic` task
"Report on Vector's Environment" will improve the test-harness work with regard
to increasing reliability but is not a blocking task. The criterion "Discover
Tolerances" will help establish a baseline for test-harness tests introduced in
"Expand Test Coverage" but we might also take some arbitrary commit as "base"
for new tests.

### Ad Hoc Sprint

Performance work is a matter of being familiar with the code base under
examination, it's goals and the context in which it runs. I'd like for everyone
working on Vector performance as a full-time concern to have an initial ramp
sprint.

Completion criteria: My aim in my own ramp sprint will be to deepen my
understanding of the code base by pursuing low-hanging fruit present in
flamegraph traces of the project for representative workload, see test-harness
section below, pair with people on their PRs and stub out documentation notes on
performance work in Vector.

### Criterion

Our criterion benchmarks establish non-integrated and partially-integrated
performance profiles for Vector components. Our existing strategy here is ideal
and we intend no major changes.

#### Push Into the Critical PR Path

Our existing criterion benchmarks do act as a check on our PRs but are not
mandatory. The feedback loop today is very long, roughly an hour to two hours
and would provide significant friction for contributors if the check were
mandatory presently. We must reduce the total time the criterion check takes and
then, once done, push the criterion benchmarks into the critical path for PRs.

Completion criteria: Without reducing the scope or reliability of our criterion
benchmarks we intend to complete the full run in a reasonable amount of time,
say 10 minutes. This implies giving more hardware to the CI process tasked with
benchmarking and splitting the load across multiple machines.

#### Expand Coverage

Our existing approach to criterion is in a good state. We will be well-served by
expanding this coverage. At a minimum every source/sink/transform must have a
benchmark that establishes its bytes-per-second throughput. Additional
benchmarks that help us determine whether regressions have been introduce and
where are, of course, for the common good. The source/sink/transforms should be
tackled in priority order.

Completion criteria: Once every source/sink/transform is covered with criterion
benchmarks to the standard where we can declare its bytes-per-second throughput
and guard against regressions in the same this task will be completed.

#### Discover Tolerances

As we expand our criterion coverage we will be able to declare the
bytes-per-second throughput of our major components. We will include these
tolerances in our documentation _and_ use these tolerances to calculate the cost
to run Vector for certain workloads, given the relevant IO and host details. For
instance, how much does a `syslog -> json_parser -> elasticsearch` Vector cost
to run if the user is running in GCP us-central1 and expects 1Gb/second of
syslog per host?

Completion criteria: When a source/sink/transform is covered by criterion
benchmarks and has a machine discovered throughput characterization we will
document this for our end users. This task is completed when all tolerances are
known, documented and find use in higher level documentation for users.

### Test-Harness

The test-harness is our place for integrated performance testing. The results
are unreliable, infrequent and, while we gain a non-trivial portion of system
information from the test subjects we do not necessarily have _actionable_
information from the subjects. These are the concerns we have to repair.

In roughly this order we will do the following.

#### Focus our Efforts

We will pull a single _performance test_ to focus our efforts on. This test will
be `file -> json_parser -> http`. We will deterministically generate json lines,
parse and then emit to an HTTP sink for a period of 1 hour. The file source and
http sink will be our **primary sources of measure**. The source will record its
**per second** throughput (lines, bytes) in a manner that DOES avoid coordinated
omission. Files will be rotated by the generator after +100Mb have been written
to them. Rotated files will be rotated for five iterations, then deleted. The
HTTP sink will record the **per second** throughput (lines, bytes) received
from Vector. No attempt will be made to verify the correctness of
transmission. Each peer will run in the same VM but isolated into different
cgroups with isolated resources.

Completion criteria: A new performance test `file -> json_parser -> http` will
be runnable in a reproducible VM environment for 1 hour. The data from the file
source and HTTP sink will be collected and shipped off-system. The test-harness
will be rigged to run this test on-demand for a given nightly vector.

#### Run Nightly

We will rig up an additional system to run the test from the previous section
nightly, shipping the data as before.

Completion criteria: There will be a process in place to run suitable
performance tests nightly, using the most recent nightly builds of vector.

#### Reduce Noise

In order to drive stability in our results we have to reduce noise. This is done
partially by careful configuration of our VMs and elimination of unreliable VMs
before benchmarks begin -- see `diagnostic` work below -- but because some tests
will be subject to nondeterminism outside our control we'll need to adjust for
this on a test by test basis. We expect, especially for the first test, to
primarily adjust by the use of repeat and parallel runs. After the fact analysis
of runs can be done to summarize the result, in much the same way as our
criterion benchmarks function.

Completion criteria: We will be able to run our performance tests in a way that
reduce noise automatically, based on detected instability. Criterion is our
model. User's configuration knobs will set details surrounding max parallelism,
total runtime and etc.

#### Backfill Performance Tests

The value of our performance tests comes in their comparison with previous
results. We must have the ability to "backfill" results for new benchmarks or
for when we substantially change our benchmark approach.

Completion criteria: We will backfill our performance tests far back enough to
give a reasonable trend-line. In cases where compatibility has been broken
between vector releases we'll support alternate configuration where reasonable.

#### Expand Side-Channel Data

To this point the only data we've collected with any serious intention have been
the **primary sources of measure**. These measurements give us a notion of how
the project is trending but does not necessarily give us any guidance as to why
or what to do about it. We will collect
[perf](https://perf.wiki.kernel.org/index.php/Main_Page) data from our test
runs, with the key insights being dependent on the test. We will collect
flamegraphs of our runs. Likewise, we will collect Vector's internal telemetry
into a comparable state. We may also explore eBPF tracing to track syscall
counts.

Completion criteria: Our aim with collecting side-channel data is to provide
comparison between versions. If, for instance, the test is sensitive to context
switches how have context switches changed between versions and to what degree
can we inject new telemetry into Vector to measure (and reduce) this behavior?
While collecting information is useful the main result here will be tooling to
_compare_ versions, to demonstrate change.

#### Expand Test Coverage

Once we have pursued a single test to its utmost we must expand out. The
existing pool of performance tests in test-harness are suitable for conversion,
though we might consider a new variant if user needs are evident.

Completion criteria: This work will be "complete" when we have five total tests
in the new test-harness style. Realistically we will continually add onto the
harness until such time as it no longer proves useful to us, but we gotta draw
the line somewhere.

### Pursue `diagnostic` Command

We describe in [Issue 4660](https://github.com/vectordotdev/vector/issues/4660) the
desire to pursue a diagnostic command for Vector. There are two, complementary
themes for such a tool and we will pursue them both. The ordering of these
themes will be what provides the most efficacy for our problems at hand.

#### Report on Vector's Internals

Vector maintains self-telemetry but we have a hard time getting at this
information in user deployments. We need Vector to be able to answer questions
of itself to the degree that it can report things like "the file source is
spending most of its time writing checkpoints" or similar. It's possible that we
will expose this information through other means, but the `diagnostic` will
output a normalized report for use in tickets and otherwise.

Completion criteria: This work will be complete when Vector is able to
self-telemeter and answer relevant details about potential places for resource
constraints, serialization and the like. When available this report will be
consumed by the test-harness for after-run details.

#### Report on Vector's Environment

Our users will run Vector in a wide variety of environments, which we may need
to replicate. For instance, if a user reports that Vector's file source is
unusually slow it _may_ be slow because they have an unusually slow disk or are
present in a system with low memory and high swap utilization. The `diagnostic`
will perform fundamental timing tests -- see
[above](#build-a-vector-diagnostic-sub-command) -- that will inform us about
potential weirdness. We may consider including recommendations in the output for
users. This diagnostic information will be used in test-harness to reject VMs
that would give poor data stability.

Completion criteria: This work will be complete when Vector is able to detect
key features of its environment and report on these, drawing special attention
to any aberrant details.
