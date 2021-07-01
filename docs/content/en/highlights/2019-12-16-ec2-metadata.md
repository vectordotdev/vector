---
date: "2020-03-31"
title: "New AWS EC2 Metadata Transform"
description: "Enrich your events with EC2 metadata"
authors: ["binarylogic"]
pr_numbers: [1325]
release: "0.6.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["transforms"]
  transforms: ["aws_ec2_metadata"]
aliases: ["/blog/ec2-metadata"]
---

Are your events the laughing-stock of the data warehouse? Then enrich them with
our brand spanking new [`aws_ec2_metadata` transform][docs.transforms.aws_ec2_metadata].

<!--more-->

Configuration isn't complicated, just add and hook up the transform. If you
don't want all enrichments added then white-list them with the `fields` option:

```toml
[transforms.fill_me_up]
  type = "aws_ec2_metadata"
  inputs = ["my-source-id"]
  fields = [
    "instance-id",
    "local-hostname",
    "public-hostname",
    "public-ipv4",
    "ami-id",
    "availability-zone",
    "region",
  ]
```

For more guidance get on the [reference page][docs.transforms.aws_ec2_metadata].

## Why?

Data is better when it's thicc 👌

[docs.transforms.aws_ec2_metadata]: /docs/reference/configuration/transforms/aws_ec2_metadata
