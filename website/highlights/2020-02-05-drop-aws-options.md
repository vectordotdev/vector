---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "AWS specific options have been dropped in the Elasticsearch sink"
description: "We've dropped redundant AWS options that may break backward compatibility"
author_github: "https://github.com/binarylogic"
pr_numbers: [1703]
release: "0.8.0"
importance: "low"
tags: ["type: breaking change", "provider: aws", "domain: sinks", "sink: elasticsearch"]
---

The `endpoint` and `region` options have been dropped in the [`elasticsearch`
sink][docs.sinks.elasticsearch] in favor of using the `host` option. If you
are one of the rare users that set these options please set the `host` option
to the full domain of your AWS elasticsearch cluster.


[docs.sinks.elasticsearch]: /docs/reference/sinks/elasticsearch/
