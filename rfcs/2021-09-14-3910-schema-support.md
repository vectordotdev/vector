# RFC 3910 - 2021-09-14 - Schema Support

We allow operators to define their event schema(s) ahead-of-time, allowing more
fine-gained control over the flow of events based on their shapes, and to
optimize event processing such as reducing the need for type-coercion in
VRL-powered components.

<!-- vim-markdown-toc GFM -->

* [Context (TODO)](#context-todo)
* [Cross cutting concerns](#cross-cutting-concerns)
* [Scope](#scope)
  * [In scope](#in-scope)
  * [Out of scope](#out-of-scope)
* [Pain](#pain)
* [User Experience](#user-experience)
  * [Schema Validation](#schema-validation)
  * [Invalid Events](#invalid-events)
    * [Dropping Events](#dropping-events)
    * [Accept Invalid Events](#accept-invalid-events)
    * [Dead-Letter Pipeline](#dead-letter-pipeline)
  * [Schema Use-Cases](#schema-use-cases)
    * [Implicit Schema Conversion](#implicit-schema-conversion)
    * [Remap-based Conversion](#remap-based-conversion)
      * [Type Checking Integration](#type-checking-integration)
      * [Output Schema Validation](#output-schema-validation)
      * [Output Schema Generation](#output-schema-generation)
    * [Schema Conditions](#schema-conditions)
* [Implementation (TODO)](#implementation-todo)
  * [Schema Validation](#schema-validation-1)
    * [Boot-time Validation](#boot-time-validation)
    * [Run-time validation](#run-time-validation)
  * [Schema Conversion (TODO)](#schema-conversion-todo)
  * [Condition Checking (TODO)](#condition-checking-todo)
* [Rationale](#rationale)
* [Drawbacks](#drawbacks)
  * [Performance](#performance)
  * [Complex Topology](#complex-topology)
* [Outstanding Questions (TODO)](#outstanding-questions-todo)
* [Plan Of Attack (TODO)](#plan-of-attack-todo)
* [Future Improvements (TODO)](#future-improvements-todo)

<!-- vim-markdown-toc -->

## Context (TODO)

* ...

## Cross cutting concerns

_none, yet_

## Scope

### In scope

- Add schema support to Vector.
- Add new `schema` condition type.

### Out of scope

- Hook up Vector schema support to VRL's type checking (requires separate RFC).

- Add individual schemas for known sources and sinks (requires follow-up issue
  with list of sources/sinks and links to their schema definitions).

- Support custom schemas (requires separate RFC).

- Schema-based dead-letter pipeline (requires separate RFC).

- Exposing schema metadata to operators (no need, but potentially useful in the
  future).

## Pain

When working with observability events, the shape of the event is an important
factor in the success of delivering those events. This is why VRL requires type
checking event fields at compile-time.

In larger organisations, internal systems are often maintained by different
teams, and there is likely a dependency on one or more external systems as well.
Often, these systems produce streams of observability events. These events can
have different shapes, and since the team maintaining the observability pipeline
often does not control the sources that produce the events, there is usually
a human contract between observability operators and developers of the
individual systems defining which fields should be present and what types those
fields should be.

More often than not, human contracts without a programmable guarantee falls down
at one point or another. This can happen because teams accidentally introduce
a bug, causing an event field type to change, or be missing entirely, or it can
happen due to miscommunication between teams.

Such a bug can have disastrous consequences for observability pipelines,
especially if events are streamed into systems that make it difficult to later
remove erroneously delivered events, without having to re-process the entire
event stream again.

Event schemas are one way of enforcing the contract between teams, and ensuring
that only valid events end up at their expected destination.

## User Experience

Vector operators should have the option to guarantee the shape of events based
on event schemas.

To do this, they can do one of two things.

1. For sources with a **known** schema, Vector guarantees the schema of each
   event received by that source, by implicitly enforcing the source-schema on
   those events.

   - [ ] **TODO** Which sources have a fixed/known schema?

   - [ ] **TODO** Should we ever allow operators to disable and/or change the
     schema of those sources?

2. For sources that don't have a fixed schema defined, operators can define
   their own schema — or use a schema pre-defined within Vector — to enforce the
   shape of events.

### Schema Validation

Support for schema validation is imperative to the overarching goal of schema
support in Vector. Without validation, there can be no guarantee that an event
matches the expected schema, which in turn prevents other systems within Vector
to rely on the information that an even adheres to a given schema.

Schema validation is an expensive operation, however. For every incoming event,
the entire shape of the event has to be compared against a pre-defined schema.
While we can optimize this as much as possible, the inherent cost of walking an
event and doing comparisons cannot be ignored, and has to be accepted as an
inherent cost of implementing schema support.

As developers, we instinctively search for solutions to this problem, such as
sampled schema validation, to limit the impact on performance, but because all
internal systems will be built _assuming_ schema adherence is guaranteed, there
can be no room for error, nor for shortcuts that give you mostly statistically
correct results, but not always.

Even so, aside from the impact on the internal systems, if we promote strict
schema adherence, the expectation is that no badly formatted events will end up
at the configured destination.

Because of this fact, this RFC proposes one solution to start with: you either
_enable_ schema support, and accept the inherent performance cost, or you
_disable_ support, cannot be guaranteed any schema adherence, but keep the most
amount of performance. In the future, alternative "strategies" might be added,
to find a middle-ground, but this would likely require a new RFC, given the
mentioned implications.

### Invalid Events

For any source that has a schema configured, it can either **drop** or
**accept** invalid events.

#### Dropping Events

When a source is configured to drop invalid events, and an event does not match
the schema, the event won't be allowed to proceed to the next component in the
pipeline.

When this happens, the event is dropped, and an internal event is triggered
indicating the failed schema validation.

#### Accept Invalid Events

If the source is configured to accept invalid events, it internally exposes
itself as exporting events matching both the operator-configured, and the "any"
schema. This is the least-restrictive schema definition.

Allowing invalid events to be ingested has consequences. For one, downstream
components can no longer rely on strict schema enforcement (in a technical sense
they can, since all events adhere to the "any" schema). For example, for
implicit schema transformations, the "any" schema provides no event structure
guarantees, and thus no transformation can be done from events matching this
schema.

On the surface, the "any" schema thus becomes a useless piece of information.
However, operators can choose to re-route mismatched events, by adding a `route`
transform in-between. This transform can route any events matching the more
strict schema to one lane, and all other events matching the "any" schema to
another lane.

#### Dead-Letter Pipeline

In the future, dropped events can be picked up by a [dead-letter pipeline][dlq],
but this is out-of-scope for this RFC.

In a sense, accepting invalid events, combined with the `route` transform,
already lets you manually implement a dead-letter pipeline.

[dlq]: https://github.com/vectordotdev/vector/issues/1772

### Schema Use-Cases

Once an event is known to have a fixed schema, operators can use this
information in one of three ways.

#### Implicit Schema Conversion

They can use automated schema conversion capabilities built into Vector to
accept an event with schema `A` from one source, and have it be exported using
schema `B` from a compatible sink.

This only works for schemas for which Vector has built-in knowledge on how to
convert from one schema to another. For example, schema A might use `timestamp`
as the time stamp field, and B uses `ts`, this conversion happens behind the
scenes. A more involved conversion could mean that one time stamp format uses
RFC3339, while another uses Unix time.

This last example shows that there are details that we still need to flesh out.
For example, do we allow loss of data when converting events between schema's,
as long as this is documented (e.g. RFC3339 supports a wider range of dates than
Unix time does).

To use this automated conversion, all the operator needs to do, is connect the
relevant source (optionally through a pipeline of transforms) to the final sink.
Vector can detect at boot which schema the sink is going to receive based on the
pipelines involved, check whether the sink type has a schema attached to it, and
whether that schema can be converted to, from the received event schemas. If it
can't, Vector refuses to boot, and explains the reasoning for not booting.

#### Remap-based Conversion

If automated schema conversion is not available, or unwanted, operators can
leverage the `remap` transform to do a manual schema conversion.

There are multiple ways this transform ties into the proposed schema support.

##### Type Checking Integration

First, when an event received by the transform has a schema attached to it, the
VRL compiler uses this schema during type checking. Before this RFC, all event
fields started as "any" type (e.g. unknown). Once this RFC is implemented,
schema type information can be used to inform the VRL compiler. This means that
operators no longer have to do the type enforcement themselves, meaning
a program such as this:

```coffee
.http_code = to_int(.http_code) ?? 0
if (.http_code >= 400 && .http_code < 500) {
  # ...
}
```

Can now be simplified to:

```coffee
if (.http_code >= 400 && .http_code < 500) {
  # ...
}
```

Because the compiler infers from the schema that the `http_code` field must
always be an integer.

It goes without saying that allowing this to exists means that the VRL compiler
is entirely dependent on Vector providing the **correct** schema details for any
given event. If it does not, and the received event is shaped differently from
what the VRL compiler expected based on the schema definition it received,
undefined runtime errors will occur when running the VRL program against the
event.

##### Output Schema Validation

Additionally, if the operator specified the required output schema of the
transform, Vector uses VRL's type checking to ensure that whatever the VRL
program produces once it runs to completion, matches the expected output schema.

If the output does not match the expected schema, Vector won't boot. This
ensures that operators can update VRL programs while knowing that Vector will
continue to validate the final output, preventing them from introducing a bug in
the program that invalidates the schema adherence.

```toml
[transforms.remap]
  type = "remap"
  inputs = []

  # Ensure any event produced by this transform adheres to my "custom" schema.
  schema.out.type = "custom"
  schema.out.file = "./my_schema.json"

  # Given VRL's compile-time checking, we can check at Vector boot-time whether
  # the output of this transform matches the expected schema. If it doesn't,
  # Vector fails to boot.
  source = """
    . = { "foo": "bar" }
  """
```

##### Output Schema Generation

Even if the operator _does not_ specify a required output schema, Vector still
built up the schema internally based on the output types of the VRL program.
This schema can then be used by other transforms and sinks within the same
pipeline.

If the output of a VRL program matches a _known_ schema (e.g. `elasticsearch`),
then events emitted by this transform automatically get that schema type
attached to them, and if it doesn't match any known schemas, then they get
a `custom` schema attached to them.

Either way, using the `remap` transform in any pipeline automatically results in
a schema being attached to the resulting events, courtesy of the VRL compiler.

```toml
[transforms.remap]
  type = "remap"
  inputs = []

  # The output schema of this transform is defined to have a "foo" field with
  # a string-value.
  source = """
    . = { "foo": "bar" }
  """
```

#### Schema Conditions

Finally, schemas can be used in "condition-based" transforms.

A condition-based transform is one that performs an action on the received event
based on a condition. Currently those include the `route`, `reduce`, `sample`,
and `filter` transforms.

Currently, these transforms take either the `vrl` condition type, or the
deprecated `check_fields` type. After implementing this RFC, a new `schema` type
is added, which allows applying conditional logic based on the schema attached
to each individual event.

For example, you could route events to different sinks based on the schema of
each event, or filter out events of a given schema.

## Implementation (TODO)

### Schema Validation

Because schema validation is expensive, we want to limit the times we have to
validate events against a given schema, and we want to push as much validation
out of the hot-path (e.g. when Vector boots, not when individual events are
processed).

To achieve this, validation is split between two stages; **boot-time
validation** and **run-time validation**.

#### Boot-time Validation

It's impossible to validate against a schema when you don't know the shape of
the event yet. This means that to ensure an event adheres to a given schema, we
_have_ to apply the validation at run-time, in the hot path.

However, what we can do, is to ensure that whatever pipelines are configured,
_given the assumption that the incoming event will be validated against
a schema_, use that fact to then determine the correctness of the pipeline at
boot-time, and reject any incorrectly configured schema validation set-up.

Here's how this would work:

1. For each **source**, the implicit or explicit schemas defined for that source
   are tracked. Any source that doesn't have a schema defined is considered to
   expose "any" schema.

2. Each downstream **transform** passes along the merged set of the schema
   definitions of their upstream components to their child components. That is,
   if transform `C` receives events from sources `A` and `B`, and `A` exports
   events adhering to schema `1` and `2` and `B` for schema `2` and `3`, then
   transform `C` defines itself as exporting events adhering to schema `1`, `2`
   and `3`.

3. Transforms _can_ overwrite this default behavior. For example, the `route`
   transform could route events based on schema `1` differently than `2`. Given
   that the route transform compiles down to one transform per lane, each lane
   will have a different schema output defined.

   Similarly, the `remap` transform defines its own output schema(s), based on
   information from the VRL compiler.

4. Each **sink** has an implicit or explicit export schema defined. It too
   creates a merged set of its input components similar to transforms, but
   instead of producing a list of exported schemas, it checks that the input
   schemas can be automatically converted to the required export schema as
   defined in the sink.

5. If anything in the above chain is invalid, Vector returns an error, and won't
   boot.

By doing this, We've covered (_almost, see below_) all cases related to schema
validation, conversions and routing, except for the initial validation happening
when events are received by a source.

- [ ] **TODO** Give examples on how we're going to expand the different
      component traits.

#### Run-time validation

This is where run-time validation is required.

Because sources are the _only_ type of component that can ingest events from
external places, it is also the only place that requires us to do run-time
validation. All other invariants have already been covered by the boot-time
validations.

Run-time schema validations are expensive, but there's no way to avoid them. For
the first implementation of this RFC, we're going to naively check each
individual event using an existing Rust-based JSON-schema library. In the
future, we potentially want to look into combining event decoding and schema
validation into a single step, but this will likely mean either misusing
Serde's `Deserialize` trait, or not using Serde at all.

 - [ ] **TODO** discuss/benchmark potential solutions.

Once an event is validated at the source, a new piece of metadata is added to
the event, indicating against which schema ID the event was validated. This
metadata can be used by downstream components to perform actions on the event,
based on the schema. For example, any condition-based transform can use the
schema ID to pass or fail the condition (e.g. schema-based routing).

### Schema Conversion (TODO)

To allow schema's to be converted between each other, we produce a system that
can determine at boot-time whether one schema can be converted into another.
This is done by defining a set of rules and restrictions to which both schemas
needs to adhere (e.g. a schema for a schema).

For example, to support going from schema `A` to `B` [...]

To support implicit schema conversions, Vector will maintain a list of
conversions we support.

### Condition Checking (TODO)

...

## Rationale

Correctness is an important aspect in the world of observability data. Human
contracts can get you so far, but schema validation is the end-all-be-all when
it comes to reaching 100% correctness in your events processing pipeline.

## Drawbacks

### Performance

The biggest drawback to this feature is the impact it has on Vector's
performance. Validating each individual event against one or more schemas is
expensive, there's no way around that. There is a clear trade-off to be made
here between being 100% correct through schema validation, or sticking to human
contracts and keeping the existing performance Vector offers.

### Complex Topology

Another potential drawback is the complexity this can introduce in configuring
pipelines. When enabling schema validation, the potential for many existing
pipelines to be rejected is there. Enabling (and configuring) schema support is
done by the operator themselves, so it's a concious choice, but still one that
adds maintenance overhead (but reduces the potential for bugs, of course).

## Outstanding Questions (TODO)

...

## Plan Of Attack (TODO)

- [ ] ...

## Future Improvements (TODO)

- [ ] schema-validation transform, for when you want to have a source accept
  "any" event, but then later in the pipeline (after routing/filtering/etc) want
  to apply an actual schema validation.
