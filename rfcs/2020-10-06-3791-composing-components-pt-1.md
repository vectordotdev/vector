# RFC 3791 - 2020-10-06 - Composing Components: Part 1

Vector is designed to be very modular, and the current tool for composing those
modules is the TOML config file. This gives users a great deal of flexibility,
but it can require configurations that are a bit verbose and require more of
users than other pre-built, specific solutions.

One way that Vector could get some of the best of both worlds would be to make
it easy to create pre-built "chunks" of config that users could configure as
normal components. These would be bundles of lower-level components wired
together with adjusted default values for the specific use case.

## Scope

This RFC focuses on enabling rapid development of "composed" sources (e.g. NGINX
logs) within our existing architecture. A more complete solution for composing
arbitrary components is deferred to a later RFC.

## Motivation

We need a way to quickly assemble Vector components that address specific use
cases. This will allow us to improve ease of use without spending significant
development time on each individual use case. It will allow us to focus
development time on reuseable components without forcing users to do the work of
assembling them from scratch.

## Internal Proposal

There are multiple levels at which we could implement this type of
functionality:

1. Manually implement new component as config facade over one existing component
2. Manually implement new component as config facade over one source and one
   codec transform
3. Manually implement new component as config expanding to arbitrary pipeline of
   components
4. Automatically derive new component from data describing arbitrary pipeline of
   components

We currently are at level (1), where we can do things like implement the Humio
sink as a wrapper around the existing Splunk HEC sink.

The next simplest is level (2). While it's not implemented yet, we do have
existing plans to introduce the idea of a codec attached to sources. This would
allow users to directly configure how to parse the incoming data as part of the
source config itself. With that feature implemented, it would be relatively
straightforward to do something similar to level (1) but expanding to both
a source and an included codec.

Level (3) becomes more complicated. We currently have a limited ability for
transforms to expand to multiple transforms via `TransformConfig::expand`, and
this could theoretically be generalized to include sources and sinks as well.
The main problem is that this does not mesh well with the config traits as they
currently exist and the API can be confusing. To do this properly would likely
involve deeper changes to the config traits to better support this kind of
staged building.

Finally, layer (4) would allow defining these compositions via TOML instead of
Rust code. This is somewhat similar to the idea of snippets that has been
floated previously, but with a few key differences. The main one is that they
would be built directly into Vector at compile time instead of loaded at
runtime. This means they would need to be integrated into our build process and
changing them would require recompiling Vector. They would also require
a sufficiently general composition API to be exposed via TOML, which would be
difficult to come up with for such a wide variety of potential pipelines. For
these two reasons, I doubt that level (4) is worthwhile right now (this could
change when/if we have more data-driven config definition in general).

My proposal is that we initially focus on level (2) while collecting data on use
cases that require level (3). It is my assumption that the largest number of
these types of composed components will be similar to the example of the NGINX
source. We will want to combine an existing source (file) with an existing
transform (regex or grok parser) and provide NGINX-specific default values for
each. Focusing on these simpler cases will dramatically decrease how much
complexity we need to add before being able to reap the value.

## Rationale

This set of changes unblocks the most user-facing value with the least required
investment, and it does so without compromising future plans for deeper
architectural changes.

## Plan of Attack

- [ ] Implement `TransformFn` from the [Architecture
    RFC](https://github.com/vectordotdev/vector/blob/master/rfcs/2020-06-18-2625-architecture-revisit.md),
    switch non-task transforms to it
- [ ] Add `Vec<dyn TransformFn>` field to `Pipeline`
- [ ] Implement composed sources as facades that prepend the relevant `TransformFn`
    to the `Pipeline` passed to `SourceConfig::build`
- [ ] Move `event_processed` internal events to topology wrappers instead of
    components themselves to avoid double counting or incorrect tagging (likely
    within `impl Transform for TransformFn` for now)

Then later we can choose to push towards level (3) as needed:

- [ ] Make `TransformConfig::expand` into first-class stage, splitting the
    existing config `build` methods
- [ ] Allow new expansion stage to work for all components, not just transforms
- [ ] Consider introducing more fine-grained internal component types designed
    to be composed into user-facing sources, transforms, and sinks
