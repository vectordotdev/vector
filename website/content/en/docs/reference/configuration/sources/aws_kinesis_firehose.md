---
title: AWS Kinesis Firehose
description: Collect logs from [AWS Kinesis Firehose](https://aws.amazon.com/kinesis/data-firehose)
kind: source
---

## Requirements

{{< component/requirements >}}

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Context

{{< snippet "context" >}}

### Forwarding CloudWatch Logs events

The `aws_kinesis_firehose` source is the recommended source for ingesting logs from [AWS Cloudwatch Logs][cloudwatch_logs] via [AWS Cloudwatch Logs subscriptions][cloudwatch_logs_subscription]. To set this up:

1. Deploy Vector with a publicly exposed HTTP endpoint using this source. We recommend using the [`aws_cloudwatch_logs_subscription_parser`][subscription_parser] transform to extract the log events. Make sure to set the [`access_key`](#access_key) to secure this endpoint. Your configuration might look something like this:

    ```toml
    [sources.firehose]
    type = "aws_kinesis_firehose"
    address = "127.0.0.1:9000"
    access_key = "secret"

    [transforms.cloudwatch]
    type = "aws_cloudwatch_logs_subscription_parser"
    inputs = ["firehose"]

    [sinks.console]
    type = "console"
    inputs = ["cloudwatch"]
    encoding.codec = "json"
    ```

1. Create a Kinesis Firewatch delivery stream in the region where the Cloudwatch Logs groups exist that you want to ingest.

1. Set the stream to forward to your Vector instance via its HTTP endpoint destination. Make sure to configure the same [`access_key`](#access-key) you set earlier.

1. Set up a [Cloudwatch Logs subscription][cloudwatch_logs_subscription] to forward the events to your delivery stream.

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[cloudwatch_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[cloudwatch_logs_subscription]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Subscriptions.html
[subscription_parser]: /docs/reference/configuration/transforms/aws_cloudwatch_logs_subscription_parser
