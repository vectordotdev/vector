---
title: Concepts
description: The fundamental Vector concepts. A great place to start learning about Vector.
---

<SVG src="/optimized_svg/concepts_687_357.svg" />

It's worth getting familiar with the basic concepts that comprise Vector as they
are used throughout the documentation. This knowledge will be helpful as you
proceed and is also cool to brag about amongst your friends.

## Components

"Component" is the generic term we use for [sources](#sources),
[transforms](#transforms), and [sinks](#sinks). You compose components to create
pipelines, allowing you to ingest, transform, and send data.

<Jump to="/components/">View all components</Jump>

### Sources

Vector wouldn't be very useful if it couldn't ingest data. A "source" defines where Vector
should pull data from, or how it should receive data pushed to it. A pipeline
can have any number of sources, and as they ingest data they proceed to
normalize it into [events](#events) \(see next section\). This sets the stage
for easy and consistent processing of your data. Examples of sources include
[`file`][docs.sources.file], [`syslog`][docs.sources.syslog],
[`StatsD`][docs.sources.statsd], and [`stdin`][docs.sources.stdin].

<Jump to="/docs/reference/sources/">View all sources</Jump>

### Transforms

A "transform" is responsible for mutating events as they are transported by
Vector. This might involve parsing, filtering, sampling, or aggregating. You can
have any number of transforms in your pipeline and how they are composed is up
to you.

<Jump to="/docs/reference/transforms/">View all transforms</Jump>

### Sinks

A "sink" is a destination for [events][docs.data-model]. Each sink's
design and transmission method is dictated by the downstream service it is
interacting with. For example, the [`socket` sink][docs.sinks.socket] will
stream individual events, while the [`aws_s3` sink][docs.sinks.aws_s3] will
buffer and flush data.

<Jump to="/docs/reference/sinks/">View all sinks</Jump>

## Events

Data, such as logs and metrics, that passes through Vector is known as an
"event". Events are explained in detail in the [data model][docs.data-model]
section.

<Jump to="/docs/about/data-model/">View data model</Jump>

## Pipelines

A "pipeline" is the end result of connecting [sources](#sources),
[transforms](#transforms), and [sinks](#sinks). You can see a full example of a
pipeline in the [configuration section][docs.setup.configuration].

<Jump to="/docs/setup/configuration/">View configuration</Jump>

[docs.setup.configuration]: /docs/setup/configuration/
[docs.data-model]: /docs/about/data-model/
[docs.sinks.aws_s3]: /docs/reference/sinks/aws_s3/
[docs.sinks.socket]: /docs/reference/sinks/socket/
[docs.sources.file]: /docs/reference/sources/file/
[docs.sources.statsd]: /docs/reference/sources/statsd/
[docs.sources.stdin]: /docs/reference/sources/stdin/
[docs.sources.syslog]: /docs/reference/sources/syslog/
