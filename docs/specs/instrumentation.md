# Instrumentation Specification

This document specifies Vector's instrumentation for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Introduction](#introduction)
1. [Naming](#naming)
   1. [Metric naming](#metric-naming)

<!-- /MarkdownTOC -->

## Introduction

Vector's runtime behavior is expressed through user-defined configuration files
intended to be written directly by users. Therefore, the quality of Vector's
configuration largely affects Vector's user experience. This document aims to
make Vector's configuration as high quality as possible in order to achieve a
[best in class user experience][user_experience].

## Naming

### Metric naming

For metric naming, Vector broadly follows the
[Prometheus metric naming standards]. Hence, a metric name:

* MUST only contain ASCII alphanumeric, lowercase, and underscores
* MUST follow the `<name>_<unit>_[total]` template
  * `name` is one or more words that describes the measurement (e.g., `memory_rss`, `requests`)
  * `unit` MUST be a single [base unit] in plural form, if applicable (e.g., `seconds`, `bytes`)
  * Counters MUST end with `total` (e.g., `disk_written_bytes_total`, `http_requests_total`)
* MUST NOT contain a namespace since the `internal_metrics` source sets a configurable namespace
* SHOULD be broad in purpose and use use tags to differentiate characteristics of the measurement (e.g., `host_cpu_seconds_total{cpu="0",mode="idle"}`)

[Prometheus metric naming standards]: https://prometheus.io/docs/practices/naming/
[single base unit]: https://en.wikipedia.org/wiki/SI_base_unit
