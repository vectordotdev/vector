---
description: Core Vector concepts
---

# Concepts

![][assets.components]

Before you begin, it's useful to become familiar with the basic concepts that comprise Vector. These concepts are used throughout the documentation and are helpful to understand as you proceed. They are ordered by their natural progression.

## Sources

The purpose of Vector is to collect data from various sources in various shapes. Vector is designed to pull _and_ receive data from these sources depending on the source type. As Vector ingests data it proceeds to normalize that data into a [record](#records) \(see next section\). This sets the stage for easy and consistent processing of your data. Examples of sources include [`file`][docs.sources.file], [`syslog`][docs.sources.syslog], [`tcp`][docs.sources.tcp], and [`stdin`][docs.sources.stdin].

{% page-ref page="../usage/configuration/sources/" %}

## Transforms

A "transform" is anything that modifies an event or the stream as a whole, such as a parser, filter, sampler, or aggregator. This term is purposefully generic to help simplify the concepts Vector is built on.

{% page-ref page="../usage/configuration/transforms/" %}

## Sinks

A sink is a destination for [events][docs.data_model#event]. Each sink's design and transmission method is dictated by the downstream service it is interacting with. For example, the [`tcp` sink][docs.sinks.tcp] will stream individual records, while the [`aws_s3` sink][docs.sinks.aws_s3] will buffer and flush data.

{% page-ref page="../usage/configuration/sinks/" %}


[assets.components]: ../assets/components.svg
[docs.data_model#event]: ../about/data-model#event
[docs.sinks.aws_s3]: ../usage/configuration/sinks/aws_s3.md
[docs.sinks.tcp]: ../usage/configuration/sinks/tcp.md
[docs.sources.file]: ../usage/configuration/sources/file.md
[docs.sources.stdin]: ../usage/configuration/sources/stdin.md
[docs.sources.syslog]: ../usage/configuration/sources/syslog.md
[docs.sources.tcp]: ../usage/configuration/sources/tcp.md
