---
date: "2020-07-14"
title: "New compression options for some sinks"
description: "Stuff more data down a smaller hose in less time for cheaper."
authors: ["hoverbear"]
hide_on_release_notes: false
pr_numbers: [2953, 2637, 2679, 2682]
release: "0.10.0"
badges:
  type: "new feature"
  sinks: ["aws_kinesis_firehose", "aws_kinesis_streams", "aws_s3", "humio_logs"]
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
