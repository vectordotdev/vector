---
title: Prometheus Exporter
kind: sink
---

## Warnings

{{< component/warnings >}}

## Configuration

{{< component/config >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Histogram buckets

Choosing the appropriate buckets for Prometheus histograms is a complicated point of discussion. The [Histograms and Summaries Prometheus guide][guide]) provides a good overview of histograms, buckets, summaries, and how you should think about configuring them. The buckets you choose should align with your known range and distribution of values as well as how you plan to report on them. The aforementioned guide provides examples on how you should align them.

#### Default buckets

The [`buckets`](#buckets) option defines the global default buckets for histograms. These defaults are tailored to broadly measure the response time (in seconds) of a network service. Most likely, however, you will be required to define buckets customized to your use case.

### Memory usage

Like other Prometheus instances, the `prometheus_exporter` sink aggregates metrics in memory which keeps the memory footprint to a minimum if Prometheus fails to scrape the Vector instance over an extended period of time. The downside is that data will be lost if Vector is restarted. This is by design of Prometheus' pull model approach, but is worth noting if restart Vector frequently.

### State

{{< snippet "stateless" >}}

[guide]: https://vector.dev/docs/reference/configuration/sinks/prometheus_exporter/(urls.prometheus_histograms_guide
