---
date: "2021-07-23"
title: "Datadog Search Syntax support"
description: "Use Datadog Search Syntax in `filter` transforms and VRL"
authors: ["leebenson"]
pr_numbers: [7837, 8370],
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: new feature
  providers: ["datadog"]
  domains: ["vrl", "filter transform"]
---

This release adds support for [Datadog Search Syntax](datadog_seach_syntax), a
query language based loosely on [Lucene](lucene) and designed for parsing
Datadog log lines.

This allows you to filter log lines  akin to the experience offered by [Datadog
Live Tailing](datadog_live_tailing).

There's two ways you can incorporate Search Syntax into your Vector workflow:

## 1. As a `filter` transform condition

Assuming the following `vector.toml`:

```toml
[api]
  enabled = true

[sources.gen1]
  type = "generator"
  format = "json"
  batch_interval = 0.2

[transforms.filter]
  type = "filter"
  inputs = ["gen1"]
  condition.type = "datadog_search"
  condition.source = "*mopper"

[sinks.blackhole]
  type = "blackhole"
  inputs = ["gen1"]
  print_amount = 1000
```

Running `vector tap` will yield log messages affixed with "mopper" - e.g:

```
{"message":"{\"host\":\"16.203.222.220\",\"user-identifier\":\"meln1ks\",\"datetime\":\"26/Jul/2021:10:33:21\",\"method\":\"POST\",\"request\":\"/apps/deploy\",\"protocol\":\"HTTP/1.1\",\"status\":\"501\",\"bytes\":29112,\"referer\":\"https://for.net/booper/bopper/mooper/mopper\"}","timestamp":"2021-07-26T09:33:21.908370Z"}
{"message":"{\"host\":\"16.203.222.220\",\"user-identifier\":\"meln1ks\",\"datetime\":\"26/Jul/2021:10:33:21\",\"method\":\"POST\",\"request\":\"/apps/deploy\",\"protocol\":\"HTTP/1.1\",\"status\":\"501\",\"bytes\":29112,\"referer\":\"https://for.net/booper/bopper/mooper/mopper\"}","timestamp":"2021-07-26T09:33:21.908370Z"}
{"message":"{\"host\":\"26.224.165.77\",\"user-identifier\":\"ahmadajmi\",\"datetime\":\"26/Jul/2021:10:33:22\",\"method\":\"POST\",\"request\":\"/apps/deploy\",\"protocol\":\"HTTP/2.0\",\"status\":\"501\",\"bytes\":6119,\"referer\":\"https://make.de/observability/metrics/production\"}","timestamp":"2021-07-26T09:33:22.126398Z"}
```

Just set the following two lines in your `filter` configuration:

```toml
condition.type = "datadog_search"
condition.source = "<Datadog Search Syntax query goes here>"
```

## 2. In VRL, using `match_datadog_query`

The new [match_datadog_query](match_datadog_query) function returns `true` if a
Search Syntax query is found in the provided object.

Examples:

```
# Each of these return true

match_datadog_query({"message": "contains this and that"}, "this OR that")
match_datadog_query({"custom": {"name": "vector"}}, "@name:vec*")
match_datadog_query({"tags": ["a:x", "b:y", "c:z"]}, s'b:["x" TO "z"]')
```

## Use-case

The purpose of this function is to support Datadog log lines, which are encoded
to support Datadog-specific concepts such as reserved fields, facets and tags.

Because of this, `match_datadog_query` is not intended as a general purpose
Lucene-like syntax, but rather as a specialist function for  use alongside
Datadog log payloads.

Here are a few common ways this distinction manifests:

* Bare search terms such as `"find me"` will search for the default fields in
  the following order: `message`, `custom.error.message`, `custom.error.stack`,
  `custom.title`, `_default_`.

* Default search fields perform 'full text' / word boundary searches. e.g.
  `hello` will match a log line containing `{"message": "say hello"}`.

* All other fields will perform full-field searches. e.g. `say:hello` will match
  `{"tags": ["say:hello"]}` but not `{"tags": ["say:hello there"]}`.

* Tag searches are prefixed with either `tags` or the tag name - e.g.
  `host:"google.com"` or `tags:host`.

* Facets can be searched by prefixing with `@` - e.g. `@name:John`. This would
  return true with a payload containing `{"custom": {"name": "John"}}`.

* Range searches can be performed inclusively using `[1 TO 10]` (e.g. 1-10,
  inclusive) or exclusively on the upper/lower bounds with `{1 TO 10}` (e.g.
  2-9).

* Facets are compared numerically when either operand is a numeral or treated as
  a string otherwise. All other fields are treated as strings. This can lead to
  surprising behavior - e.g. `@value:>10` will find `{"custom": {"value": 15}}`,
  but `value:>15` would be the tag `value` and searched by UTF-8 order - e.g.
  "100" would match, but not "2".

See the [Datadog Search Syntax docs page](datadog_search_syntax) for an
introductory overview of how this syntax is used in practice at Datadog.

## Future work

As the integration between Vector and Datadog deepens, we will be introducing
new ways to interact with Datadog payloads. Search Syntax is likely to be used
in more places.

This is our first step to introducing cross-platform support using a shared
query language.

Watch future release notes for more details.

[lucene]: https://lucene.apache.org/ [datadog_search_syntax]:
https://docs.datadoghq.com/logs/explorer/search_syntax/ [datadog_live_tailing]:
https://docs.datadoghq.com/logs/explorer/live_tail/ [match_datadog_query]:
/docs/reference/vrl/functions/#match_datadog_query