---
title: AWS S3
description: Collect logs from [AWS S3](https://aws.amazon.com/s3)
kind: source
---

## Requirements

{{< component/requirements >}}

## Configuration

{{< component/config >}}

## Environment variables

{{< component/env-vars >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}


## How it works

### AWS authentication

{{< snippet "aws/auth" >}}

### Context

{{< snippet "context" >}}

### Handling events from the `aws_s3` source

This source behaves in a similar fashion to the [`file`] source in that it outputs one event per line (unless the [`multiline`](#multiline) configuration option is used). You may need to use [transforms] to parse the data. To parse VPC flow logs sent to S3, for example, you can use the [`tokenizer`][tokenizer] transform:

```toml title="vector.toml"
[transforms.flow_logs]
type = "tokenizer" # required
inputs = ["s3"]
field_names = ["version", "account_id", "interface_id", "srcaddr", "dstaddr", "srcport", "dstport", "protocol", "packets", "bytes", "start", "end", "action", "log_status"]

types.srcport = "int"
types.dstport = "int"
types.packets = "int"
types.bytes = "int"
types.start = "timestamp|%s"
types.end = "timestamp|%s"
```

To parse AWS load balancer logs, you can use the [`regex_parser`] transform:

```toml title="vector.toml"
[transforms.elasticloadbalancing_fields_parsed]
type = "regex_parser"
inputs = ["s3"]
regex = '(?x)^
        (?P<type>[\w]+)[ ]
        (?P<timestamp>[\w:.-]+)[ ]
        (?P<elb>[^\s]+)[ ]
        (?P<client_host>[\d.:-]+)[ ]
        (?P<target_host>[\d.:-]+)[ ]
        (?P<request_processing_time>[\d.-]+)[ ]
        (?P<target_processing_time>[\d.-]+)[ ]
        (?P<response_processing_time>[\d.-]+)[ ]
        (?P<elb_status_code>[\d-]+)[ ]
        (?P<target_status_code>[\d-]+)[ ]
        (?P<received_bytes>[\d-]+)[ ]
        (?P<sent_bytes>[\d-]+)[ ]
        "(?P<request_method>[\w-]+)[ ]
        (?P<request_url>[^\s]+)[ ]
        (?P<request_protocol>[^"\s]+)"[ ]
        "(?P<user_agent>[^"]+)"[ ]
        (?P<ssl_cipher>[^\s]+)[ ]
        (?P<ssl_protocol>[^\s]+)[ ]
        (?P<target_group_arn>[\w.:/-]+)[ ]
        "(?P<trace_id>[^\s"]+)"[ ]
        "(?P<domain_name>[^\s"]+)"[ ]
        "(?P<chosen_cert_arn>[\w:./-]+)"[ ]
        (?P<matched_rule_priority>[\d-]+)[ ]
        (?P<request_creation_time>[\w.:-]+)[ ]
        "(?P<actions_executed>[\w,-]+)"[ ]
        "(?P<redirect_url>[^"]+)"[ ]
        "(?P<error_reason>[^"]+)"'
field = "message"
drop_failed = false

types.received_bytes = "int"
types.request_processing_time = "float"
types.sent_bytes = "int"
types.target_processing_time = "float"
types.response_processing_time = "float"

[transforms.elasticloadbalancing_url_parsed]
type = "regex_parser"
inputs = ["elasticloadbalancing_fields_parsed"]
regex = '^(?P<url_scheme>[\w]+)://(?P<url_hostname>[^\s:/?#]+)(?::(?P<request_port>[\d-]+))?-?(?:/(?P<url_path>[^\s?#]*))?(?P<request_url_query>\?[^\s#]+)?'
field = "request_url"
drop_failed = false
```

[file]: /docs/reference/configuration/sources/files
[tokenizer]: /docs/reference/configuration/transforms/tokenizer
[transforms]: /docs/reference/configuration/transforms
