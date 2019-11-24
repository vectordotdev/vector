---
title: Concepts
description: Core Vector concepts
---

import SVG from 'react-inlinesvg';

<SVG src="/img/concepts.svg" />

Before you begin, it's useful to become familiar with the basic concepts that
comprise Vector. These concepts are used throughout the documentation and are
helpful to understand as you proceed.

## Components

"Component" is the generic term we use for [sources](#sources),
[transforms](#transforms), and [sinks](#sinks). You compose components to create
pipelines, allowing you to ingest, transform, and send data.

import Jump from '@site/src/components/Jump';

<Jump to="/components">View all components</Jump>

### Sources

The purpose of Vector is to collect data from various sources in various shapes. Vector is designed to pull _and_ receive data from these sources depending on the source type. As Vector ingests data it proceeds to normalize that data into a [record](#records) \(see next section\). This sets the stage for easy and consistent processing of your data. Examples of sources include [`file`][docs.sources.file], [`syslog`][docs.sources.syslog], [`tcp`][docs.sources.tcp], and [`stdin`][docs.sources.stdin].

<Jump to="/docs/reference/sources">View all sources</Jump>

### Transforms

A "transform" is anything that modifies an event or the stream as a whole, such as a parser, filter, sampler, or aggregator. This term is purposefully generic to help simplify the concepts Vector is built on.

<Jump to="/docs/reference/transforms">View all transforms</Jump>

### Sinks

A sink is a destination for [events][docs.data_model#event]. Each sink's design and transmission method is dictated by the downstream service it is interacting with. For example, the [`tcp` sink][docs.sinks.tcp] will stream individual records, while the [`aws_s3` sink][docs.sinks.aws_s3] will buffer and flush data.

<Jump to="/docs/reference/sources">View all sinks</Jump>

## Events

"Events" are the generic term Vector uses to represent all data (logs and
metrics) flowing through Vector. This is covered in detail in the
[data model][docs.data-model] section.

<Jump to="/docs/about/data-model">View data model</Jump>

## Pipelines

"Pipelines" are the end result of connecting [sources](#sources),
[transforms](#transforms), and [sinks](#sinks). You can see a full example
of a pipeline in the [configuration section][docs.configuration].

<Jump to="/docs/setup/configuration">View configuration</Jump>


[docs.configuration]: /docs/setup/configuration
[docs.data-model]: /docs/about/data-model
[docs.data_model#event]: /docs/about/data-model#event
[docs.sinks.aws_s3]: /docs/reference/sinks/aws_s3
[docs.sinks.tcp]: /docs/reference/sinks/tcp
[docs.sources.file]: /docs/reference/sources/file
[docs.sources.stdin]: /docs/reference/sources/stdin
[docs.sources.syslog]: /docs/reference/sources/syslog
[docs.sources.tcp]: /docs/reference/sources/tcp
