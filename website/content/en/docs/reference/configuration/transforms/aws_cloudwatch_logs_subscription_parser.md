---
title: AWS Cloudwatch Logs Subscription Parser
kind: transform
deprecated: true
---

The `aws_cloudwatch_logs_subscription_parser` parses events from [AWS Cloudwatch Logs][cloudwatch_logs] (configured through [AWS Cloudwatch Logs subscriptions][cloudwatch_logs_subscription]) coming from the [`aws_kinesis_firehose`][kinesis_firehose] source.

## Warnings

{{< component/warnings >}}

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### State

{{< snippet "stateless" >}}

### Structured log events

Note that the events themselves aren't parsed. If they are structured data, we recommend passing them through a parsing transform.

[cloudwatch_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[cloudwatch_logs_subscription]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Subscriptions.html
[kinesis_firehose]: /docs/reference/configuration/sources/aws_kinesis_firehose
