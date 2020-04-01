---
last_modified_on: "2020-03-31"
id: ec2-metadata
title: "EC2 Metadata Enrichments"
description: "Enrich your events with EC2 metadata"
author_github: https://github.com/Jeffail
tags: ["type: announcement", "domain: transforms", "transform: ec2_metadata"]
---

Are your events the laughing-stock of the data warehouse? Then enrich them with
our brand spanking new [`aws_ec2_metadata` transform][docs.transforms.aws_ec2_metadata].

<!--truncate-->

Configuration isn't complicated, just add and hook up the transform. If you
don't want all enrichments added then whitelist them with the `fields` option:

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


[docs.transforms.aws_ec2_metadata]: /docs/reference/transforms/aws_ec2_metadata/
