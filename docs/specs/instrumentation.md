# Instrumentation Specification

This document specifies Vector's instrumentation for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

- [Introduction](#introduction)
- [Naming](#naming)
  - [Event naming](#event-naming)
  - [Metric naming](#metric-naming)
- [Emission](#emission)
  - [Batching](#batching)
  - [Events](#events)
    - [Error](#error)
    - [EventsDropped](#eventsdropped)

<!-- /MarkdownTOC -->

## Introduction

Vector's telemetry drives various interfaces that operators depend on to manage
mission critical Vector deployments. Therefore, Vector's telemetry should be
high quality and treated as a first class feautre in the development of Vector.
This document strives to guide developers into achieving this.

## Naming

### Event naming

Vector implements an event-driven instrumentation pattern ([RFC 2064]) and
event names MUST adhere to the following rules:

* MUST only contain ASCII alphanumeric and lowercase characters
* MUST be in [camelcase] format
* MUST follow the `<Namespace><Noun><Verb>[Error]` template
  * `Namespace` - the internal domain the event belongs to (e.g., `Component`, `Buffer`, `Topology`)
  * `Noun` - the subject of the event (e.g., `Bytes`, `Events`)
  * `Verb` - the past tense verb describing when the event occured (e.g., `Received`, `Sent`, `Processes`)
  * `[Error]` - if the event is an error it MUST end with `Error`

### Metric naming

Vector broadly follows the [Prometheus metric naming standards]:

* MUST only contain ASCII alphanumeric, lowercase, and underscore characters
* MUST be in [snakecase] format
* MUST follow the `<namespace>_<name>_<unit>_[total]` template
  * `namespace` - the internal domain that the metric belongs to (e.g., `component`, `buffer`, `topology`)
  * `name` - is one or more words that describes the measurement (e.g., `memory_rss`, `requests`)
  * `unit` - MUST be a single [base unit] in plural form, if applicable (e.g., `seconds`, `bytes`)
  * Counters MUST end with `total` (e.g., `disk_written_bytes_total`, `http_requests_total`)
* SHOULD be broad in purpose and use tags to differentiate characteristics of the measurement (e.g., `host_cpu_seconds_total{cpu="0",mode="idle"}`)

## Emission

### Batching

For performance reasons, as demonstrated in [pull request #8383],
instrumentation SHOULD be batched whenever possible:

* Telemtry SHOULD emit for entire event batches, not each individual event.
  [RFC 9480] describes Vector's batching strategy.
* Benchmarking SHOULD prove that batching produces performance benefits.
  [Issue 10658] could eliminate the need to batch for performance improvements.

### Events

Instrumentation SHOULD be event-driven ([RFC 2064]), where individual events
serve as the vehicle for internal telemetry, driving the emission of metrics
and logs. This organizes Vector's telemetry, making it easier to manage and 
catalogue. Metrics and logs SHOULD NOT be emitted directly except for where it
is otherwise impossible to emit Vector's events, such as in an external crate
that cannot import Vector's events.

#### Error

An `<Name>Error` event MUST be emitted when an error occurs. Errors that are
retriable and possible recoverable MUST be distinguished from errors that are
not:

* Properties
  * `retriable` - If the operation that produced the error is retriable. This
    indicates that the error operation might recover and determines if this
    error should be a warning or an error when emitting logs and metrics. For
    example, a failed HTTP request that will be retried.
  * `error_code` - An error code for the failure, if applicable.
    * SHOULD only be specified if it adds additional information beyond
      `error_type`.
    * The values for `error_code` for a given error event MUST be a bounded set
      with relatively low cardinality because it will be used as a metric tag.
      Examples would be syscall error code. Examples of values that should not
      be used are raw error messages from `serde` as these are highly variable
      depending on the input. Instead, these errors should be converted to an
      error code like `invalid_json`.
  * `error_type` - The type of error condition. MUST be one of the types listed
    in the `error_type` enum list in the cue docs.
  * `stage` - The stage at which the error occurred. MUST be one of `receiving`,
    `processing`, or `sending`.
  * If any of the above properties are implicit to the specific error
    type, they MAY be omitted from being represented explicitly in the
    event fields. However, they MUST still be included in the emitted
    logs and metrics, as specified below, as if they were present.
* Metrics
  * MUST include the defined properties as tags.
  * If `retriable` is `true`, MUST increment `<namespace>_warnings_total` metric.
  * If `retriable` is `false`, MUST increment `<namespace>_errors_total` metric.
* Logs
  * MUST log a descriptive, user friendly error message that sufficiently
    describes the error.
  * MUST include the defined properties as key-value pairs, except `message`.
  * If `retriable` is `true`, MUST log a message at the `warning` level.
  * If `retriable` is `false`, MUST log a message at the `error` level.
  * SHOULD be rate limited to 10 seconds.
* Events
  * MUST emit an [`EventsDropped`] event if the error results in dropping events,
    or the error itself MUST meeting the `EventsDropped` requirements.

#### EventsDropped

An `<Namespace>EventsDropped` event must be emitted when events are dropped.
If events are dropped due to an error, then the error event should drive the
emission of this event, meeting the following requirements:

* Properties
  * `intentional` - Distinguishes if the events were dropped intentionally. For
    example, events dropped in the `filter` transform are intentionally dropped,
    while events dropped due to an error in the `remap` transform are
    unintentionally dropped.
* Metrics
  * MUST increment the `<namespace>_discarded_events_total` counter by the
    number of events discarded.
  * MUST NOT increment this metric if retrying the operation will preserve the
    event, such as retrying delivery in sinks.
  * MUST include the `intentional` property as a tag.
* Logs
  * MUST log a `<count> events [un]intentionally dropped due to <reason>`
    message. `<reason>` MUST be a descriptive, user-friendly message that helps
    the user understand why their data was dropped.
  * If `intentional` is `true`, MUST log at the `debug` level.
  * If `intentional` is `false`, MUST log at the `error` level.


[camelcase]: https://en.wikipedia.org/wiki/Camel_case
[`EventsDropped`]: #EventsDropped
[Issue 10658]: https://github.com/vectordotdev/vector/issues/10658
[Prometheus metric naming standards]: https://prometheus.io/docs/practices/naming/
[pull request #8383]: https://github.com/vectordotdev/vector/pull/8383/
[RFC 2064]: https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
[RFC 9480]: https://github.com/vectordotdev/vector/blob/master/rfcs/2021-10-22-9480-processing-arrays-of-events.md
[single base unit]: https://en.wikipedia.org/wiki/SI_base_unit
[snakecase]: https://en.wikipedia.org/wiki/Snake_case
