---
last_modified_on: "2020-07-14"
$schema: "/.meta/.schemas/highlights.json"
title: "New compression options for some sinks"
description: "Stuff more data down a smaller hose in less time for cheaper."
author_github: "https://github.com/hoverbear"
hide_on_release_notes: false
pr_numbers: [2953,2637,2679,2682]
release: "0.10.0"
tags: ["type: new feature", "sink: aws_s3", "sink: humio_logs", "sink: aws_kinesis_firehose", "sink: aws_kinesis_streams"]
---

Several sinks, including most AWS sinks as well as [Humio][urls.humio] and [New Relic][urls.new_relic] have had compression options added.

## Enabling Compression

Compression is opt-in. Make the following changes in your `vector.toml` file:

```diff title="vector.toml"
  [sinks.little-pipe]
    type = "aws_cloudwatch_metrics" # required
    inputs = ["big-firehose"] # required
    healthcheck = true # optional, default
    namespace = "service" # required
    region = "us-east-1" # required, required when endpoint = ""
+   compression = "gzip" # optional, default none
```

[urls.humio]: https://humio.com
[urls.new_relic]: https://newrelic.com/
