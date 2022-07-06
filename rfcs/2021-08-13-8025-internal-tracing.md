# RFC #8025 - 2021-08-12 - Internal Tracing

This RFC discusses an integration with the Datadog APM product for Vector's
internal tracing data, with the goal of enabling faster and more accurate
diagnosis of performance issues in deployed environments.

## Status

The team decided not to move forward with this change. Implementing tracing
within Vector is not a worthy goal in itself, and there is a clearer, cheaper
path to addressing the pain points laid out in this RFC via more granular timing
metrics.

That being said, the team recognizes that tracing has the potential to provide
unique observability into Vector internals, just not necessarily in the form
laid out in this RFC. We will continue to collect implementation ideas and use
cases, as well as keep an eye on the tracing ecosystem. When a strong case can
be made, we will revisit this topic.

## Context

- Existing experimental implementation ([#7929](https://github.com/vectordotdev/vector/pull/7929))
- Internal instrumentation epic ([#3900](https://github.com/vectordotdev/vector/issues/3900))

## Scope

### In scope

- Instrumenting components with tracing spans
- Sending internal tracing spans to Datadog APM via the Datadog Agent
- An opt-in system for internal soak tests and experimental use in customer environments

### Out of scope

- Sending internal traces anywhere other than Datadog APM
- Sending internal traces directly to Datadog without a local agent
- An always-on tracing subsystem that is enabled by default in all customer deployments
- General support for sending tracing data through Vector (i.e. trace data type, sources and sinks)

## Pain

- Debugging Vector performance in users environments requires a lot of back and forth
- The data available for debugging Vector is relatively high level and doesn't always map clearly to specific components and operations

## Proposal

### User Experience

Vector will gain a new `--enable-datadog-tracing` flag that users can enable at
start time. When enabled, Vector will forward internal tracing spans to a local
Datadog Agent with the APM feature enabled. A valid API key will be required to
start the agent, but no additional configuration of Vector itself is required.

### Implementation

#### Overview

Vector already has some very basic trace-style instrumentation via the
[`tracing`](https://docs.rs/tracing/) crate that we currently use primary for
logging. This RFC proposes that we expand that instrumentation to include more
granular spans around the existing events (i.e. `info!` and friends, not Vector
events).

With those additional spans in place, we can rely on `tracing` subscribers from
existing libraries to forward those spans to a local Datadog agent.

#### Instrumentation

How we instrument Vector components with spans should be driven by the kinds of
questions we'll be looking to answer with the resulting data. In particular, the
focus should be on the types of questions that are difficult to answer with our
current tools (metrics, logs, and sometimes Datadog's native profiler).

A typical performance investigation starts at a very high level and works its
way down into specifics. The first step tends to be identifying which component
is the bottleneck. This is often done with some intuition and/or experimentation
(e.g. "Lua tends to be slower, try dropping it and see what happens"), and the
new utilization metrics are designed to help with exactly this.

The step after that is a bit fuzzy, however. You could jump straight to
profiling, but it has a few weaknesses:

1. It's not something that can easily be run in customer environments
1. The focus on CPU time vs wall clock time means it can miss issues like bad
   rate limits, waiting on downstream components, slow IO operations or
   syscalls, etc
1. The output tends to require interpretation by an experienced engineer, and
   doesn't always indicate clearly where time is being spent from a Vector
   perspective

These limitations provide a good outline of the areas where tracing can cover
the gap between profiling and metrics: **a component-focused breakdown of where
time is being spent, with specific emphasis on off-CPU operations**.

For a concrete example, consider the file source. It has a run loop that
consists at a high level of discovering new files, reading from tracked files,
sending the output upstream, and sleeping if all files were idle. This implies
a good starting point for instrumenting it with spans would be to add an overall
loop iteration span, and child spans for each of those four primary stages. Even
with only those in place, the tracing data is immediately more useful than
either metrics or profiling for a number of real world issues we've encountered.
For stages with multiple interesting sub-stages (e.g. collecting paths vs
fingerprinting during file discovery), one additional layer of child spans can
be added to further increase the amount of information available for debugging.

In addition to the question of what spans to add, we need to determine the log
level at which these spans should operate. Because we've been using `tracing`
primarily as a traditional logging library to this point, leveling spans and
having those levels coupled to logging statements can feel a bit weird. To keep
things relatively simple, **we propose that the spans discussed here be given
a default level of `INFO`**. This will make them available without the
additional noise that comes with increased logging levels, and should not cause
a significant performance impact given the relatively broad spans we're
recommending (i.e. no high-throughput, per-event spans).

One final concern when instrumenting with spans is the length of the span's
lifetime. In early experiments, we ran into issues where extremely long-lived
spans (e.g. covering the full runtime of a component) are opened and collect
data continuously, never flushing because the span does not end. This lead to
extreme amount of memory growth. The problem was greatly exaggerated by enabling
`TRACE`-level logging, which resulted in an enormous number of log events
collecting within the span. To address this, **we proposing pushing the
outermost spans down from the lifetime of a component to an "iteration" of
a component**. This will depend greatly on the specifics of each component, but
the file source above can serve as an example. We also propose keeping these
spans at the `INFO` level, where our logging is actually pretty quiet and there
is far less potential for accumulating memory even if spans last longer than
intended.

In summary:

- Spans should be component-focused, breaking down their operations by stages with at least two levels of hierarchy but up to three levels for components with more complex internals:
    1. Single "iteration" of operation, on the order of seconds, as a baseline
    2. High level stages of operation within an iteration, such as reading,
       processing, sending
    3. Interesting sub-stages of each operation, with a focus on units of work
       with significant off-CPU time
- Spans should be instantiated at the `INFO` log level

#### Forwarding

For the mechanics of actually sending span data to Datadog, we propose relying
on a couple of existing libraries to handle virtually all of the heavy lifting.
It's possible that we'll want to extend or replace them in the future, but
currently the [`tracing-opentelemetry`](https://docs.rs/tracing-opentelemetry/)
and [`opentelemetry-datadog`](https://docs.rs/opentelemetry-datadog) crates
combine to fulfill this function to a satisfactory degree.

## Rationale

### Why is this change worth it?

With a relatively small investment of effort, the system described here would be
able to deliver significant, Vector-specific insight into performance directly
from deployed environments. The type of instrumentation needed is already
present within Vector, just underutilized, and it very neatly fills a gap
between high-level metrics and low-level CPU profiling.

### What is the impact of not doing this?

Performance investigations would continue to rely on a combination of intuition
and expert-level tooling to narrow down problems reported by users. Instead of
having data directly indicating that, for example, a component is spending all
of its time fingerprinting files, we would need to deduce that from other
sources of information.

### How does this position us for success in the future?

While this is far from an endgame tracing system, it puts a number of important
components into place and helps us start gaining experience with them. Over
time, as we fix issues and gain confidence in this relatively simple setup, we
can add additional layers of more detailed spans, measure and optimize overhead,
experiment with options like sampling, etc. If all goes well, we could get to
a significantly more advanced system with small iterative steps from this base.

## Drawbacks

### Why should we not do this?

An argument could be made that the combination of coupling our logging and
tracing, and the relative immaturity of the community crates for shipping traces
to Datadog will result in a somewhat fragile system. This has shown itself to
a degree with the memory growth issues in early testing, and it's not entirely
clear how we'd go about observing the trace forwarding libraries themselves. The
counter argument would be that this use of off-the-shelf components is exactly
what makes this initial experimentation cheap, we have a plan to avoid the
issues we've run into, and we'll have plenty of time to flush out issues in our
own soak test environments before recommending this to users.

Another, somewhat more vague objection could involve the runtime characteristics
of Vector. Tracing largely comes from request-response oriented systems, where
there is a very clear unit of work around which to wrap your tracing spans. As
a streaming system, Vector does not have as clear a boundary for those spans.
This mismatch has shown itself as part of the memory growth issue with
long-lived spans. It's possible that some components may not have a clear
concept of a single "iteration" around which we can wrap a base span, and we'll
have a hard time instrumenting them in a way that is both logical and
practically useful.

### What kind on ongoing burden does this place on the team?

The ongoing maintenance burden should be quite minimal. The extent of code
changes would be the addition of some small span instantiations and
instrumentation, but nothing that would require threading through existing
interfaces or reorganizing any abstractions.

## Prior Art

There is very little prior art available for tracing streaming system, but we
are largely just using existing off-the-shelf components. We've not opted to
invent anything particularly new for Vector.

## Alternatives

### Event-focused spans

The primary alternative implementation that's been discussed is to have spans be
event-focused rather than component-focused. This would involve stashing a root
span instance in a Vector event's metadata, and creating child spans to
represent each stage of its processing. This would provide an end-to-end view of
an event's journey through Vector, showing where the event itself spent time as
opposed to where each component is spending time. While this would produce some
interesting data, it would be significantly more complex and manual to
implement, and it's not clear that the data would actually be more useful in
addressing performance issues.

### Non-`tracing` spans

Another alternative would be to avoid the coupling between tracing spans and
logs, and use a library other than `tracing` for this functionality. This may
provide some benefit in reducing confusion about log levels, accumulating events
on spans, etc, but has the downside of requiring the introduction of a second
system for instrumentation. It would also be rather confusing to use the
`tracing` crate, but not for tracing, especially since it is one of the most
widely used and well-engineered crates for this purpose.

### More granular metrics

Perhaps the most compelling alternative is to tracing as a whole. Instead of
adding an additional type of signal to Vector's observability, we could invest
further into out internal metrics with a specific focus on addressing the pain
points laid out in this RFC. Most of the missing signal is around timing of
operations and their relative share of wall clock time. This is something that
could be captured reasonably well with timing histograms, with minimal runtime
cost and no additional moving parts.

The primary disadvantages of metrics in relation to traces for this use case are
as follows:

1. The lack inherent relations (à la parent and child spans) and their
   corresponding visualizations means it would take more interpretation and
   external knowledge to derive the same signal.

1. Their aggregated nature would put a limit on the level of detail (e.g. no
   file name field on a `read` span from the file source).

1. Collecting timings directly is likely to require more explicit
   instrumentation code than simply adding spans.

### Do nothing

As discussed in the Rationale section, not addressing this pain at all would
leave us with a meaningful gap in our observability. There would be no immediate
dire consequences, but we would expect to continue sinking more engineering time
into debugging customer issues than we would if this data were easily available.

## Outstanding Questions

- What is an acceptable performance overhead for enabling tracing in user environments?
- Are component-focused spans acceptable, or are there cases where event-focused spans would be clearly superior?
- Are we comfortable with the proposed methods of avoiding the previous memory growth issue?

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues
after the RFC is approved:

- [ ] Adjust existing spike-level implementation to fully match what's described here
- [ ] Instrument enough components to fully cover a chosen soak test scenario
- [ ] Enable tracing in that soak test scenario, gathering data on performance impact
- [ ] Open issues to add spans to the remaining components, organize and prioritize implementation

Note: The final step is not necessarily required, and we could take the stance
that this kind of instrumentation can happen as it is needed.

## Future Improvements

- Document specific performance impact of tracing based on measurements in our soak testing infrastructure
- Improve observability of trace shipping infrastructure itself
- Add more granular but more expensive span data at log levels like `DEBUG`
- Enable sampling and options helping us towards a potential always-on system
- Remove requirement of running a local Datadog agent
