---
title: Elasticsearch
kind: sink
---

## Requirements

{{< component/requirements >}}

## Configuration

{{< component/config >}}

## Telemetry

{{< component/config >}}

## How it works

### AWS authentication

{{< snippet "aws/auth" >}}

### Buffers and batches

{{< snippet "buffers-and-batches" >}}

### Conflicts

Vector [batches](#buffers-and-batches) data flushes it to [Elasticsearch's `_bulk` API endpoint][bulk_api]. By default, all events are inserted via the [`index`](#index) action which will update documents if an existing one has the same [`id`](#id). If [`bulk_action`](#bulk_action) is configured with `create`, Elasticsearch will not replace an existing document and instead return a conflict error.

### Data streams

By default, Vector uses the [`index`](#index) action with Elasticsearch's Bulk API. To use [data streams][data_streams], [`bulk_action`](#bulk_action) must be configured with the `create` option.

### Health checks

{{< snippet "health-checks" >}}

### Partial failures

By default, Elasticsearch allows partial bulk ingestion failures. This is typically due to type Elasticsearch index mapping errors, where data keys are not consistently typed. To change this behavior please refer to Elasticsearch's [`ignore_malformed` setting][ignore_malformed].

### Partitioning

{{< snippet "partitioning" >}}

### Rate limits and adaptive concurrenct

{{< snippet "arc" >}}

### Retry policy

{{< snippet "retry-policy" >}}

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[bulk_api]: https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html
[data_streams]: https://www.elastic.co/guide/en/elasticsearch/reference/current/data-streams.html
[ignore_malformed]: https://www.elastic.co/guide/en/elasticsearch/reference/current/ignore-malformed.html
