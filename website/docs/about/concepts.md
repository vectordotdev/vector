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

<Jump to="/components">View All Components</Jump>

### Sources

The purpose of Vector is to collect data from various sources in various shapes. Vector is designed to pull _and_ receive data from these sources depending on the source type. As Vector ingests data it proceeds to normalize that data into a [record](#records) \(see next section\). This sets the stage for easy and consistent processing of your data. Examples of sources include [`file`][docs.sources.file], [`syslog`][docs.sources.syslog], [`tcp`][docs.sources.tcp], and [`stdin`][docs.sources.stdin].

<Jump to="/docs/components/sources">View All Sources</Jump>

### Transforms

A "transform" is anything that modifies an event or the stream as a whole, such as a parser, filter, sampler, or aggregator. This term is purposefully generic to help simplify the concepts Vector is built on.

<Jump to="/docs/components/transforms">View All Transforms</Jump>

### Sinks

A sink is a destination for [events][docs.data_model#event]. Each sink's design and transmission method is dictated by the downstream service it is interacting with. For example, the [`tcp` sink][docs.sinks.tcp] will stream individual records, while the [`aws_s3` sink][docs.sinks.aws_s3] will buffer and flush data.

<Jump to="/docs/components/sinks">View All Sinks</Jump>


[docs.data_model#event]: /docs/about/data-model#event
[docs.sinks.aws_s3]: /docs/components/sinks/aws_s3
[docs.sinks.tcp]: /docs/components/sinks/tcp
[docs.sources.file]: /docs/components/sources/file
[docs.sources.stdin]: /docs/components/sources/stdin
[docs.sources.syslog]: /docs/components/sources/syslog
[docs.sources.tcp]: /docs/components/sources/tcp
