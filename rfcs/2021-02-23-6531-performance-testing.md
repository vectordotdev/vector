# RFC 6531 - 2021-02-23 - Performance Testing

This RFC documents the technical details of our existing performance testing
process and describes avenues for future work to improve over the state of the
art in this project.

* [Summary](#summary)
* [Motivation](#motivation)
* [Motivating Examples](#motivating-examples)
  * [Regression from Increased Instrumentation](#regression-from-increased-instrumentation)
  * [musl libc Release Builds](#musl-libc-release-builds)
  * [


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
trace overhead even when trace logs were flagged off.

### musl libc Release Builds

PR: https://github.com/timberio/vector/issues/2030

Introduction of [musl libc](https://musl.libc.org/) regressed performance on
tcp-to-tcp benchmark from 0.7.0 to 0.8.0 in double digit percentages. This was
caught by the [test-harness](https://github.com/timberio/vector-test-harness/)
as the PR documents but only after the releases were cut.

### `metrics` Crate Upgrade Regresses Benchmarks

Issue: https://github.com/timberio/vector/issues/6412
PR: https://github.com/timberio/vector/pull/6217#issuecomment-766435360

Upgrading the `metrics` crate to `v0.13.1` regresses our criterion benchmarks by
20%.

As a Datadog product Vector will be a key component of customer’s on-prem
deployments and must scale to their needs.

Today much of Vector’s customer feedback falls into two categories:

Why does Vector not light every CPU available to it and
why, for a particular configuration, does Vector not achieve a particular throughput?

The workflow for addressing these questions is relatively high-touch and the second is impacted by accidental regressions in Vector performance, as well. We intend to pursue a strategy of three interlocked processes:

We will make performance testing a first class concern in the Vector project, identifying defects as close to their introduction as possible.
We will improve Vector’s diagnosis tooling to reduce the time to debug and repair detected issues.
When defects inevitably make their way into a release we will emphasize on the use of these diagnostic tools to retrieve relevant information from the user, rolling what we’ve learned here back into the performance tests of Vector.

The first and second concerns form a tight, iterative feedback loop, isolated in the development process. The three parts in total are a longer, iterative feedback loop, involving the customer. In each stage we will emphasize standardization, education and repeatability.
