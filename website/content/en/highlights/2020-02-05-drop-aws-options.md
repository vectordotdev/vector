---
date: "2020-04-15"
title: "AWS specific options have been dropped in the Elasticsearch sink"
description: "We've dropped redundant AWS options that may break backward compatibility"
authors: ["binarylogic"]
pr_numbers: [1703]
release: "0.8.0"
hide_on_release_notes: true
badges:
  type: "breaking change"
  providers: ["aws"]
  domains: ["sinks"]
  sinks: ["elasticsearch"]
---

The `endpoint` and `region` options have been dropped in the [`elasticsearch`
sink][docs.sinks.elasticsearch] in favor of using the `host` option.

## Upgrade Guide

```diff title="vector.toml"
 [sinks.es]
   type = "elasticsearch"
-  endpoint = "http://my-domain.us-east-1.es.amazonaws.com"
-  region = "us-east-1"
+  host = "http://my-domain.us-east-1.es.amazonaws.com"
```

You can find your AWS ES domain in the AWS console. Simply provide the full
domain URL as the `host` value.

[docs.sinks.elasticsearch]: /docs/reference/configuration/sinks/elasticsearch/
