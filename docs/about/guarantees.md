---
description: An in-depth look into Vector's delivery guarantees
---

# Guarantees

Vector was designed with a strong focus on reliable and performant data delivery, providing you with the levers necessary to meet the requirements for your use case. This document will cover the delivery guarantees Vector can make, the caveats, and the various configuration options available to optimize towards performance or reliability.

## At Least Once Delivery

At least once delivery guarantees that an [event](concepts.md#events) received by Vector will be delivered at least once to the configured destination\(s\). While rare, it is possible for an event to be delivered more than once \(see the [Does Vector support exactly once delivery](guarantees.md#does-vector-support-exactly-once-delivery) FAQ below\).

### Quick Start

1. Enable [on-disk buffers](../usage/configuration/sinks/buffer.md) for each sink that you want to have this guarantee.
2. Use [sources and sinks that are capable](guarantees.md#support-matrix) of achieving this guarantee \(see table below\)

### How It Works

At least once delivery is achieved by [configuring on-disk buffers](../usage/configuration/sinks/buffer.md) for each of your sinks. The coupling of buffers with sinks allows you to choose guarantees on a per-sink basis. For example, it might make more sense to implement on-disk buffers for an archiving sink, but not for a short-term searching sink used solely for diagnostic purposes. In addition, due to the nature of certain sources and sinks, at least once delivery simply is not possible. We've provided a [support matrix](guarantees.md#support-matrix) below showing the guarantee each sink and source supports.

## Best Effort Delivery

Best effort delivery has no guarantees and means that Vector will make a best effort to deliver each event. This means it is possible for an event to not be delivered. For most, this is sufficient in the observability use case and will afford you the opportunity to optimize towards performance and reduce operating cost. For example, you can stick with [in-memory buffers](../usage/configuration/sinks/buffer.md#in-memory) \(default\), instead of enabling [on-disk buffers](../usage/configuration/sinks/buffer.md#on-disk), for a roughly [3X throughput increase](../usage/configuration/sinks/buffer.md#performance).

## Support Matrix

The following matrix outlines the guarantee support for each [sink](../usage/configuration/sinks/) and [source](../usage/configuration/sources/).

{% hint style="info" %}
It is possible that a sink or source may not be listed here. For clarity, each sink and source will list document it's guarantee in it's specific documentation page.
{% endhint %}

| Name | At Least Once | Best Effort |
| :--- | :---: | :---: |
| \`\`[`aws_cloudwatch_logs` sink](../usage/configuration/sinks/aws_cloudwatch_logs.md) | ✓ |  |
| \`\`[`aws_kinesis` sink](../usage/configuration/sinks/aws_kinesis_streams.md) | ✓ |  |
| \`\`[`aws_s3` sink](../usage/configuration/sinks/aws_s3.md) | ✓ |  |
| \`\`[`console` sink](../usage/configuration/sinks/console.md) | ✓ |  |
| \`\`[`elasticsearch` sink](../usage/configuration/sinks/elasticsearch.md) | ✓ |  |
| \`\`[`file` sink](../usage/configuration/sinks/file.md) | ✓\* |  |
| \`\`[`file` source](../usage/configuration/sources/file.md) | ✓\* |  |
| \`\`[`http` sink](../usage/configuration/sinks/http.md) | ✓ |  |
| \`\`[`kafka` sink](../usage/configuration/sinks/kafka.md) | ✓ |  |
| \`\`[`stdin` source](../usage/configuration/sources/stdin.md) | ✓ |  |
| \`\`[`splunk_hec` sink](../usage/configuration/sinks/splunk_hec.md) | ✓ |  |
| \`\`[`syslog` source via TCP](../usage/configuration/sources/syslog.md) | ✓ |  |
| \`\`[`syslog` source via UDP](../usage/configuration/sources/syslog.md) |  | ✓ |
| \`\`[`syslog` source via Unix Socket](../usage/configuration/sources/syslog.md) | ✓ |  |
| \`\`[`tcp` sink](../usage/configuration/sinks/tcp.md) |  | ✓ |
| \`\`[`tcp` source](../usage/configuration/sources/tcp.md) | ✓ |  |
| \`\`[`udp` sink](../usage/configuration/sinks/udp.md) |  | ✓ |
| \`\`[`udp` source](../usage/configuration/sources/udp.md) |  | ✓ |
| \`\`[`unix` source](../usage/configuration/sources/unix.md) | ✓ |  |
| \`\`[`vector` sink](../usage/configuration/sinks/vector.md) | ✓ |  |
| \`\`[`vector` source](../usage/configuration/sources/vector.md) | ✓ |  |

\* only if configured properly, see the associated documentation

## FAQs

### Do I need at least once delivery?

One of the unique advantages with the logging use case is that some data loss is usually acceptable. This is due to the fact that log data is usually used for diagnostic purposes and losing an event has little impact on the business. This is not to say that Vector does not take the at least once guarantee very seriously, it just means that you can optimize towards performance and reduce your cost if you're willing to accept some data loss.

### Does Vector support exactly once delivery?

No, Vector does not support exactly once delivery. There are future plans to partially support this for sources and sinks that support it \(Kafka, for example\), but it remains unclear if Vector will ever be able to achieve this. We recommend [subscribing to our mailing list](https://vectorproject.io), which will keep you in the loop if this ever changes.









