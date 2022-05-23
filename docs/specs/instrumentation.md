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
  - [Errors](#errors)
    - [Warning level](#warning-level)
    - [Error level](#error-level)
  - [Events](#events)

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
* SHOULD be broad in purpose and use use tags to differentiate characteristics of the measurement (e.g., `host_cpu_seconds_total{cpu="0",mode="idle"}`)

## Emission

### Batching

For performance reasons, as demonstrated in pull request #8383],
instrumentation SHOULD be batched whenever possible:

* Vector process batches of events ([RFC 9480]) and telemtry SHOULD emit for
  the entire batch, not each individual event.

### Errors

As described in the [events](#events) section, all errors must emit as events
to drive log and metric emission. Because errors can be transient and
recoverable, errors MUST be able to log at warning and error levels:

* MUST emit at the [error level](#error-level) if the error requires user
  attention, otherwise the error MUST emit at the [warning level](#warning-level)
  * Retryable errors only require user attention when the retry count is >= 3

#### Warning level

When an error event emits at the warning level it does not require user
attention and, therefore, MUST do the following:

* MUST log a message at the `warning` level
* MUST NOT increment the `component_errors_total`

#### Error level

When an error event emits at the error level it requires user attention and,
therefore, MUST do the following:

* MUST log a message at the `error` level
* MUST increment the `component_errors_total` by 1

### Events

Instrumentation SHOULD be event-driven ([RFC 2064]), where individual events
serve as the vehicle for internal telemtry, driving the emission of metrics
and logs. This organizes Vector's telemetry, making it easier to manage and 
catalogue. On rare occassions, metrics and logs can emit directly but MUST be
reserved for ocassions where it is impossible to emit Vector's events. For
example, emitting metrics in a library that cannot import Vector's events.

[camelcase]: https://en.wikipedia.org/wiki/Camel_case
[Prometheus metric naming standards]: https://prometheus.io/docs/practices/naming/
[Pull request #8383]: https://github.com/vectordotdev/vector/pull/8383/
[RFC 2064]: https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
[RFC 9480]: https://github.com/vectordotdev/vector/blob/master/rfcs/2021-10-22-9480-processing-arrays-of-events.md
[single base unit]: https://en.wikipedia.org/wiki/SI_base_unit
[snakecase]: https://en.wikipedia.org/wiki/Snake_case
