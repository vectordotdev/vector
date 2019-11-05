---
title: Data Model
description: A deep dive into Vector's data model
---

This document provides a deeper look at Vector's data model. Understanding this
goes a long way in properly [configuring][docs.configuration] Vector for your
use case.

## Event

To begin, all data flowing through Vector are considered an "events". The
general lifecycle of an event is demonstrated below:

![][assets.data-model]

Events must be classified as a [`log`](#log) or [`metric`](#metric) event. Each
type is described below in more detail. To better understand why Vector
classifies events, please see the "[Why not just event?](#why-not-just-events)"
FAQ.

import Field from '@site/src/components/Field';
import Fields from '@site/src/components/Fields';

## Log

A `log` event is a structured represention of a point-in-time event. It contains
an arbitrary set of fields (key/value pairs) that describe the event.

![][assets.data-model-log]

<Fields filters={true}>

<Field
  name={"host"}
  required={false}
  type="string">

### host

Represents the originating host of the log. This is commonly used in
[sources][docs.sources] but can be overridden via the `host_field` option for
relevant sources.

</Field>

<Field
  name={"message"}
  required={false}
  type="string">

### message

Represents the log message. This is the key used when ingesting raw string data.

</Field>

<Field
  name={"timestamp"}
  required={false}
  type="timestamp">

### timestamp

A normalized [Rust DateTime struct][urls.rust_date_time] in UTC.

</Field>

<Field
  name={"[key]"}
  required={false}
  type="any">

### \[key\]

In addition to the above fields, a log event can contain any amount of arbitrary
keys. Keys are typically added through [transforms][docs.transforms] when
parsing or structuring.

</Field>

</Fields>

## Metric

A `metric` event represents a numeric value that must be classified into one of
four types: `counter`, `histogram`, `gauge`, or `set`. Each are described in
more detail below.

![][assets.data-model-metric]

<Fields filters={true}>

<Field
  name={"counter"}
  required={false}
  type="struct">

### counter

<Fields filters={false}>

<Field
  name={"val"}
  required={true}
  type="double">

</Field>

</Fields>

</Field>

</Fields>

## How It Works

### Time Zones

If Vector receives a timestamp that does not contain timezone information
Vector assumes the timestamp is in local time, and will convert the timestamp
to UTC from the local time. It is important that the host system contain
time zone data files to properly determine the local time zone. This is
typically installed through the `tzdata` package. See [issue 551][urls.issue_551]
for more info.

### Timestamp Coercion

There are cases where Vector interacts with formats that do not have a formal
timestamp defintion, such as JSON. In these cases, Vector will ingest the
timestamp in it's primitive form (string or integer). You can then coerce the
field into a `timestamp` using the
[`coercer` transform][docs.transforms.coercer]. If you are parsing this data
out of a string, all Vector parser transforms include a `types` option,
allowing you to extract and coerce in one step.

### Types

#### Strings

Strings are UTF8 compatible and are only bounded by the available system
memory.

#### Ints

Integers are signed integers up to 64 bits.

#### Floats

Floats are signed floats up to 64 bits.

#### Booleans

Booleans represent binary true/false values.

#### Timestamps

Timestamps are represented as [`DateTime` Rust structs][urls.rust_date_time]
stored as UTC.

### Why Not Just Events?

Although Vector generalizes all data flowing through it as "events", we do
not expose our data structure as such. Instead, we expose the specific event
types (`log` and `metric`). Here's why:

1. We like the "everything is an event" philosophy a lot.
2. We recognize that there's a large gap between that idea and a lot of existing tooling.
3. By starting "simple" (from an integration perspective, i.e. meeting people where they are) and evolving our data model as we encounter the specific needs of new sources/sinks/transforms, we avoid overdesigning yet another grand unified data format.
4. Starting with support for a little more "old school" model makes us a better tool for supporting incremental progress in existing infrastructures towards more event-based architectures.


[assets.data-model-log]: ../assets/data-model-log.svg
[assets.data-model-metric]: ../assets/data-model-metric.svg
[assets.data-model]: ../assets/data-model.svg
[docs.configuration]: ../setup/configuration
[docs.sources]: ../components/sources
[docs.transforms.coercer]: ../components/transforms/coercer
[docs.transforms]: ../components/transforms
[urls.issue_551]: https://github.com/timberio/vector/issues/551
[urls.rust_date_time]: https://docs.rs/chrono/0.4.0/chrono/struct.DateTime.html
