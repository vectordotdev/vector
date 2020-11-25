---
last_modified_on: "2020-10-27"
$schema: ".schema.json"
title: "New AWS integration"
description: "Export observability data out of AWS with ease."
author_github: "https://github.com/binarylogic"
pr_numbers: [4101, 4779]
release: "0.11.0"
hide_on_release_notes: false
tags: ["type: new feature", "domain: sources", "platform: aws"]
---

Getting observability data out of AWS can sometimes feel like you're in a
[Rube Goldberg comic][rube_goldberg], stitching together umpteen tools
and hoping it all works in the end. We want to make this easier with Vector,
and 0.11 includes our initial efforts:

1. **A new [`aws_kinesis_firehose` source][aws_kinesis_firehose_source]**
2. **A new [`aws_cloudwatch_logs_subscription_parser` transform][aws_cloudwatch_logs_subscription_parser_transform]**
3. **A new [`aws_s3` source][aws_s3_source]**
4. **A new [`aws_sqs` sink][aws_sqs_sink]**
5. **A new [`aws_ecs_metrics` source][aws_ecs_metrics_source]**

## Get Started

To help you get started, we wrote [a guide][cloudwatch_guide] on collecting AWS
CloudWatch logs via AWS Firehose. With this setup you can send your AWS
CloudWatch logs to any supported Vector [sink][sinks].

We're eager to hear what you think about these sources! [Join our chat][chat]
and let us know.

[aws_cloudwatch_logs_subscription_parser_transform]: /docs/reference/transforms/aws_cloudwatch_logs_subscription_parser/
[aws_ecs_metrics_source]: /docs/reference/sources/aws_ecs_metrics/
[aws_kinesis_firehose_source]: /docs/reference/sources/aws_kinesis_firehose/
[aws_s3_source]: /docs/reference/sources/aws_s3/
[aws_sqs_sink]: /docs/reference/sinks/aws_sqs/
[chat]: https://chat.vector.dev
[cloudwatch_guide]: /guides/advanced/cloudwatch-logs-firehose/
[rube_goldberg]: https://en.wikipedia.org/wiki/Rube_Goldberg_machine
[sinks]: /docs/reference/sinks/
