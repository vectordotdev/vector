# RFC 5457 - <2020-12-08> - Dynamic sampling in the `sampler` transform

The addition of **dynamic sampling** capabilities (**DS** henceforth) to Vector would enable us to
cover a much broader range of sampling use cases than we currently can. A qualitatively better
sampling story would enhance Vector's usefulness in the logging domain while also potentially
putting Vector in a better position to move into domains like distributed tracing.

## Scope

This RFC proposes extensions to the [`sampler`][sampler] transform, including new configuration
parameters and the introduction of the **bucket** concept to Vector.

### Goals and non-goals

The purpose of the RFC is to achieve consensus around a set of desired features. Thus, the RFC does
*not* cover:

* Any Vector internals that would affect other components, e.g. the processing pipeline or
  configuration system
* Implementation details on the Rust side
* Finalized names for configuration parameters
* The specifics of sampling algorithms; consensus around that should be deferred to the development
  process or another RFC

### Domains

Dynamic sampling has implications for both logging and tracing. It does *not* have implications for
metrics, as metrics are already a form of sampling in themselves, e.g. counters reveal a raw number
of events while saying little about the content of those events.

This RFC, however, only covers logging and not tracing. It's possible that tracing would be
seamlessly covered by the changes proposed here but I lack the domain-specific knowledge to assert
that, so I will leave it as an open question for a future RFC.

### Feature parity

It should also be stated that this RFC to my knowledge doesn't propose any features that are not
available in other systems in the observability space. The goal is more to bring Vector in line with
what I take to be the state of the art than to push Vector into new territory.

## Background

First, some conceptual introduction. In general, sampling is the art of retaining only a subset of
an event stream based on supplied criteria. Omitting events is crucial to achieving goals like cost
cutting. But sampling needs to be done with care lest it compromise the *insight* that your data
streams can provide. Fortunately, if undertaken intelligently and with the right tools, sampling
lets you have your cake 🍰 (send fewer events downstream) and eat it too 🍴 (know what you need to
know about your systems).

Some common rules of thumb for sampling:

* Frequent events should be sampled more heavily than rare events
* Success events (e.g. `200 OK`) should be sampled more heavily than error events (e.g. `400 Bad
  Request`)
* Events closer to business logic (e.g. paying customer transactions) should be sampled less heavily
  than events related to general system behavior

Correspondingly, any robust sampling system should provide the ability to:

* Put events in desired "buckets" based on (preferably granular) criteria
* Define the sampling behavior of the resulting buckets (more on this below)

### General approaches

Sampling approaches fall into two broad categories:

* **Constant sampling** pays heed neither to the content of events nor to their frequency.
  An example would be omitting 4 in every 5 events in a stream, chosen completely at random.
* **Dynamic sampling** involves making intelligent decisions about which events to sample using:
  * The actual *content* of the events (e.g. HTTP status code, user agent, or customer plan).
  * The *frequency* at which events arise in the stream across a window of time.
  * Some combination of both.

### Dynamic sampling

The "dynamic" in dynamic sampling means that the sample rate varies in accordance with the "history"
of the event stream within the current time window. You might care enough about HTTP 500 events to
sample them only very lightly. But if these events begin to spike, they may quickly turn from signal
into noise. In that case, the sampling rate should adjust in accordance with that pattern. And not
only that, you should be able to specify how dramatically it adjusts to those changes.

## Motivation

In its current state, the [`sampler`][sampler] transform provides:

1. Constant sampling, though only implicitly. To achieve constant sampling, you need to specify a
  [`rate`][rate] but no [`key_field`][key_field]. In that case, all events are hashed the same
  and uniformly sampled at `rate`. This RFC does not propose changing this behavior.
2. Limited dynamic sampling in the form of **key-based** sampling. With the `sampler` you can:
  * Exclude events from sampling based on their actual content using [`exclude`][exclude].
  * Apply a sampling rate on a per-key basis using `key_field`. When making sampling
    decisions, the `sampler` chooses 1 out of N events with the same value for a given key.
    Thus with a `rate` of 5 and a `key_field` of `code` for HTTP status, 1 out of 5 events
    with the code 200 are sampled, 1 out of 5 with code 500, and so on.
  * A combination of exclusion and key-based sampling (with exclusion criteria applied first).

This RFC calls for far more robust dynamic sampling as laid out in the [internal
proposal](#internal-proposal).

### The `swimlanes` alternative

It should be noted here that a form of bucketing is already possible using [swimlanes], which can
route events based on their content. It's already possible to put a `sampler` transform downstream
from a `swimlanes` transform and select a `rate`, `key_field`, etc. for each resulting lane/bucket.
Requiring two separate transforms to address bread-and-butter sampling use cases, however, provides
a sub-optimal user experience. Vector users should be able to define bucket conditions and
properties within a single configuration block.

## Internal Proposal

I propose two major change to the `sampler` transform:

1. User should be able to create [named](#named-buckets) and [dynamic](#dynamic-buckets) sampling
    buckets. Named buckets are to be created using Remap conditions, dynamic buckets via
    configuration.
2. Users should be able to assign [sampling behavior](#sampling-behavior) to event buckets.

I propose related changes to the [metric output](#metrics) of the transform.

### Named buckets

Named buckets are defined using Remap conditions. All events that match the condition are placed
in the bucket when processed. Here's an example configuration with two named buckets:

```toml
[transforms.sample_customer_data]
type = "sampler"

[[bucket.less_interesting_transactions]]
condition = """
.status == 200 && !in_list(.user.plan, ["pro", "enterprise", "deluxe"]) && .transaction.accepted == true
"""
rate = 100

[[bucket.very_interesting_because_catastrophic_transactions]]
condition = """
.status == 500 && .user.plan == "deluxe" && len(.shopping_cart.items) > 10
"""
rate = 1
```

Named buckets are useful because they enable you to route events using arbitrarily complex criteria.
Any Remap expression that returns a Boolean can be used as a condition.

#### Order of application

The `sampler` transform should place events in buckets in the order in which the conditions are
specified, much like a `switch` statement in many programming languages. An event ends up
in the first bucket whose condition it matches; when a match occurs, condition checking ceases.

This raises the question of fallthrough behavior, i.e. what happens if an event doesn't meet any
condition and thus doesn't fall into a named bucket. I propose a component-level `fallthrough`
parameter with two possible values:

* `drop` means that unbucketed events are sampled at a rate of 1 and thus disappear from the stream
* `keep` means that unbucketed events aren't sampled

An alternative would be treating unbucketed events as belonging to an implicit bucket that can
itself be configured. I submit, however, that this would be sub-optimal, as Remap already enables
you to specify a named fallthrough bucket:

```toml
# Captures all unbucketed events
[[bucket.fallthrough]]
condition = """
true
"""
```

If no fallthrough behavior is specified, I propose `keep` as the default behavior, as dropping
unbucketed events by default is more likely to produce unexpected data loss for users (which is here
presumed to be a worse outcome than e.g. cost overruns).

### Dynamic buckets

With named buckets you can dictate precisely which buckets you want in advance. There are cases,
though, when you can't know in advance how many buckets you'll need because that number depends on
what your event stream looks like. This is where **dynamic buckets** come into play (this term isn't
a neologism but I have not encountered it in the observability space).

The key-based sampling that's already supported by the `sampler` transform implicitly provides
dynamic bucketing, as each newly encountered value for `key_field` creates a new "bucket." All
events in that bucket, e.g. all events with a `status_code` of `500`, are sampled at `rate`.

Here, I propose to enhance dynamic bucketing by enabling **multi-key sampling**. Instead of a single
`key_field` you can provide several using a `key_fields` list. Events with matching values across
all of the specified fields would be placed in the same bucket. A single key provides only one
"axis" for content-based bucketing.

> The `key_field` option would remain valid but could be deprecated in favor of specifying only one
> key using `key_fields`, thereby making a single key field a special case of key-based sampling.

With this configuration...

```toml
key_fields = ["username", "status"]
```

...these two events would end up in the same bucket...

```json
{
  "status": 200,
  "username": "hoverbear",
  "action": "purchase",
  "transaction": {
    "id": "a1b2c3d4"
  }
}
{
  "status": 200,
  "username": "hoverbear",
  "action": "delete_account",
  "transaction": {
    "id": "z9y8x7"
  }
}
```

...whereas these two would end up in different buckets despite the `username` matching:

```json
{
  "status": 200,
  "username": "hoverbear",
  "action": "purchase",
  "transaction": {
    "id": "a1b2c3d4"
  }
}
{
  "status": 500,
  "username": "hoverbear",
  "action": "delete_account",
  "transaction": {
    "id": "4d3c2b1a"
  }
}
```

#### Bucket explosion

Dynamic buckets bear the risk of a species of high cardinality problem. If you specify, say, 4
fields for `key_combination` and each of those fields can have many different values, you may end up
with many, many buckets and thereby high memory usage, performance degradation, etc. Controlling
bucket explosion via setting a hard limit, however, bears the downside that it's not clear which
bucketing strategy should take over if too many buckets are being created. Thus, for a first
iteration I propose adding [metrics](#metrics) to the `sampler` transform that would enable users to
keep track of bucket creation behavior to inform their decisions but not providing an explicit
lever.

#### Named + dynamic?

It's not inconceivable that named and dynamic buckets could coexist for the same event stream. It's
not clear, however, that this behavior serves any particular use case, and thus I propose initially
allowing for either named or dynamic buckets but not both. If users need to apply both approaches
to an event stream, they should use swimlanes to split the stream and apply separate `sampler`s.

### Exclusion

The `sampler` transform currently enables you to define criteria for excluding events from being
sampled (i.e. ending up in any bucket). I propose retaining the exclusion option but allowing users
to specifiy criteria via `check_fields` *or Remap* (only `check_fields` is currently available).

### Bucket behavior

With events separated into buckets you can begin specifying *how* events in those buckets are
sampled. The sections below propose per-bucket parameters. All of these parameters can apply to
named *or* dynamic buckets. For named buckets, these parameters are set with the bucket definition;
for dynamic buckets, the parameters would be set at the transform level and apply to all created
buckets.

#### `rate`

The base sampling rate for the bucket. If only this parameter is specified, the bucket is constantly
sampled at a rate of 1/N.

#### `sensitivity`

If this parameter is specified, that means that a dynamic sampling algorithm is applied to the
bucket. This determines how Vector adjusts the sample rate, using `rate` as the baseline, in light
of changes in event frequency. A sensitivity close to 0 would mean only small adjustments whereas a
sensitivity close to 1 would mean more dramatic adjustments.

> [Adaptive Request Concurrency][arc]'s `decrease_ratio` parameter serves as a rough analogy.

The parameters below only make sense if `sensitivity` is defined.

#### `max_rate` / `min_rate`

Optional parameters to keep the sample rate within hard limits.

#### `min_event_threshold`

The minimum number of events needed, within the time window, to begin adjusting the sample rate.
Below this threshold, `rate` applies. Defaults to 0, meaning no threshold.

#### `window_secs`

This specifies the length of the time window, in seconds, during which the sampling rate is
calculated. No dynamic rate can be calculated without a window. I propose 15 seconds as a default
but this can be determined during the development process.

### Metrics

I propose gathering the following additional metrics from the `sampler` transform:

* A gauge for the total number of current buckets
* A counter for the total number of buckets created

## Doc-level Proposal

More robust dynamic sampling capabilities would require the following documentation changes:

* Either update the existing `sampler` transform docs to include the new configuration parameters
  plus some additional explanation.
* A new dynamic sampling guide that walks through specific sampling use cases.
* A new "Under the Hood" page à la the one we provide for Adaptive Request Concurrency that explains
  the feature in more detail, with accompanying diagrams

## Prior Art

Other systems in the observability space do provide dynamic sampling capabilities. Their feature
sets and user interfaces can provide inspiration for dynamic sampling in Vector (and have influenced
this RFC). In terms of implementation, however, I'm unaware of a system built in Rust that provides
dynamic sampling capabilities, and a survey of the [Crates] ecosystem hasn't yielded any promising
libraries.

## Drawbacks

On the development side, providing more robust dynamic sampling would be far less trivial than e.g.
adding a new sink for a messaging service provided by `$CLOUD_PLATFORM` or a new Remap function, but
likely less labor intensive than e.g. the Adaptive Request Concurrency feature.

On the user side, the drawbacks would be largely cognitive. Concepts like bucketing and dynamic
sampling behavior are less intuitive than others in the observability realm. This is an area where
good documentation and a careful approach to terminology and developer experience would be extremely
important.

## Plan Of Attack

TBD

[arc]: https://vector.dev/blog/adaptive-request-concurrency
[crates]: https://crates.io
[exclude]: https://vector.dev/docs/reference/transforms/sampler/#exclude
[key_field]: https://vector.dev/docs/reference/transforms/sampler/#key_field
[rate]: https://vector.dev/docs/reference/transforms/sampler/#rate
[sampler]: https://vector.dev/docs/reference/transforms/sampler
[swimlanes]: https://vector.dev/docs/reference/transforms/swimlanes

