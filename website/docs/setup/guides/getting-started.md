---
title: Getting Started Guide
sidebar_label: Getting Started
description: Getting started with Vector
---

Vector is a simple beast to tame. In this guide we'll build a pipeline with some
common transformations, send an [event][docs.data-model] through it, and touch
on some basic concepts.

import Alert from '@site/src/components/Alert';
import CodeHeader from '@site/src/components/CodeHeader';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

## 1. Install Vector

If you haven't already, install Vector. Here's a script for the lazy:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh
```

Or [choose your preferred installation method][docs.installation].

## 2. Configure it

Vector runs with a [configuration file][docs.configuration] that tells it which
components to run and how they should interact. Let's create one that reads
unstructured Apache logs over TCP using a [`socket` source][docs.sources.socket]
and then writes them to an [`elasticsearch` sink][docs.sinks.elasticsearch].
We'll do this all without having to setup a local Elasticsearch cluster:

<CodeHeader text="vector.toml" />

```toml
# Consume data
[sources.foo]
  type = "socket"
  address = "0.0.0.0:9000"
  mode = "tcp"

# Write the data
[sinks.bar]
  inputs = ["foo"]
  type = "elasticsearch"
  index = "example-index"
  host = "http://10.24.32.122:9000"
```

Every component within a Vector config has an identifier chosen by you. This
allows you to specify where a sink should gather its data from (using the
`inputs` field).

That's it for our first config, if we were to run it then the raw data we
consume over TCP would be captured in the field `message`, and the object we'd
publish to Elasticsearch would look like this:

```json
{"message":"172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] \"PUT /mesh\" 406 10272","host":"foo","timestamp":"2019-05-03T13:11:48-04:00"}
```

<Alert type="info">

Pro-tip: These field names can be controlled via the
[global `log_schema` options][docs.reference.global-options#log_schema].

</Alert>

It would be much better if we could parse out the contents of the Apache logs
into structured fields.

## 3. Transform events

Nothing in this world is ever good enough for you, why should events be any
different?

Vector makes it easy to mutate events into a more (or less) structured format
with [transforms][docs.transforms]. Let's parse our logs into a structured
format by capturing named regular expression groups with a
[`regex_parser` transform][docs.transforms.regex_parser].

A config can have any number of transforms and it's entirely up to you how they
are chained together. Similar to sinks, a transform requires you to specify
where its data comes from. When a sink is configured to accept data from a
transform the pipeline is complete.

Let's place our new transform in between our existing source and sink:

<Tabs
  block={true}
  defaultValue="diff"
  values={[
    { label: 'Diff', value: 'diff', },
    { label: 'Full Config', value: 'new_result', },
  ]
}>

<TabItem value="diff">

<CodeHeader text="vector.toml" />

```diff
 # Consume data
 [sources.foo]
   type = "socket"
   address = "0.0.0.0:9000"
   mode = "tcp"


+# Structure the data
+[transforms.apache_parser]
+  inputs = ["foo"]
+  type = "regex_parser"
+  field = "message"
+  regex = '^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<mathod>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$'
+
 # Write the data
 [sinks.bar]
-  inputs = ["foo"]
+  inputs = ["apache_parser"]
   type = "elasticsearch"
   index = "example-index"
   host = "http://10.24.32.122:9000"
```

</TabItem>
<TabItem value="new_result">

<CodeHeader text="vector.toml" />

```toml
# Consume data
[sources.foo]
  type = "socket"
  address = "0.0.0.0:9000"
  mode = "tcp"

# Structure the data
[transforms.apache_parser]
  inputs = ["foo"]
  type = "regex_parser"
  field = "message"
  regex = '^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<mathod>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$'

# Write the data
[sinks.bar]
  inputs = ["apache_parser"]
  type = "elasticsearch"
  index = "example-index"
  host = "http://10.24.32.122:9000"
```

</TabItem>
</Tabs>

This regular expression looks great and it probably works, but it's best to be
sure, right?

## 4. Test it

No one is saying that unplanned explosions aren't cool, but you should be doing
that in your own time. In order to test our transform we _could_ set up a local
Elasticsearch instance and run the whole pipeline, but that's an awful bother
and Vector has a much better way.

Instead, we can write [unit tests][docs.guides.unit_testing] as part of our
config just like you would for regular code:

<Tabs
  block={true}
  defaultValue="diff"
  values={[
    { label: 'Diff', value: 'diff', },
    { label: 'Full Config', value: 'new_result', },
  ]
}>

<TabItem value="diff">

<CodeHeader text="vector.toml" />

```diff
 # Write the data
 [sinks.bar]
   inputs = ["apache_parser"]
   type = "elasticsearch"
   index = "example-index"
   host = "http://10.24.32.122:9000"
+
+[[tests]]
+  name = "test apache regex"
+
+  [[tests.inputs]]
+    insert_at = "apache_parser"
+    type = "raw"
+    value = "172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] \"PUT /mesh\" 406 10272"
+
+  [[tests.outputs]]
+    extract_from = "apache_parser"
+    [[tests.outputs.conditions]]
+      type = "check_fields"
+      "method.eq" = "PUT"
+      "host.eq" = "172.128.80.109"
+      "timestamp.eq" = "2019-05-03T13:11:48-04:00"
+      "path.eq" = "/mesh"
+      "status.eq" = "406"
```

</TabItem>
<TabItem value="new_result">

<CodeHeader text="vector.toml" />

```toml
# Consume data
[sources.foo]
  type = "socket"
  address = "0.0.0.0:9000"
  mode = "tcp"

# Structure the data
[transforms.apache_parser]
  inputs = ["foo"]
  type = "regex_parser"
  field = "message"
  regex = '^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<mathod>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$'

# Write the data
[sinks.bar]
  inputs = ["apache_parser"]
  type = "elasticsearch"
  index = "example-index"
  host = "http://10.24.32.122:9000"

[[tests]]
  name = "test apache regex"

  [[tests.inputs]]
    insert_at = "apache_parser"
    type = "raw"
    value = "172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] \"PUT /mesh\" 406 10272"

  [[tests.outputs]]
    extract_from = "apache_parser"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "method.eq" = "PUT"
      "host.eq" = "172.128.80.109"
      "timestamp.eq" = "2019-05-03T13:11:48-04:00"
      "path.eq" = "/mesh"
      "status.eq" = "406"
```

</TabItem>
</Tabs>

This unit test spec has a name, defines an input event to feed into our pipeline
at a specific transform (in this case our _only_ transform), and defines where
we'd like to capture resulting events coming out along with a condition to check
the events against.

When we run `vector test ./vector.toml` it will parse and execute our test:

```text
Running vector.toml tests
test vector.toml: test apache regex ... failed

failures:

--- vector.toml ---

test 'test apache regex':

check transform 'apache_parser' failed conditions:
  condition[0]: predicates failed: [ method.eq: "PUT" ]
payloads (events encoded as JSON):
  input: {"timestamp":"2020-02-20T10:19:27.283745Z","message":"172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] \"PUT /mesh\" 406 10272"}
  output: {"bytes_in":"656","timestamp":"2019-05-03T13:11:48-04:00","mathod":"PUT","bytes_out":"10272","host":"172.128.80.109","status":"406","user":"Bins5273","path":"/mesh"}
```

By Jove! There _was_ a problem with our regular expression! Our test has pointed
out that the predicate `method.eq` failed, and has helpfully printed our input
and resulting events in JSON format.

This allows us to inspect exactly what our transform is doing, and it turns out
that the method from our Apache log is actually being captured in a field
`mathod`.

See if you can spot the typo, once it's fixed we can run
`vector test ./vector.toml` again and we should get this:

```text
Running vector.toml tests
test vector.toml: test apache regex ... passed
```

Success! Next, try experimenting by adding more [transforms][docs.transforms] to
your pipeline before moving onto the next guide.

Good luck, now that you're a Vector pro you'll have endless ragtag groups of
misfits trying to recruit you as their hacker.


[docs.configuration]: /docs/setup/configuration/
[docs.data-model]: /docs/about/data-model/
[docs.guides.unit_testing]: /docs/setup/guides/unit-testing/
[docs.installation]: /docs/setup/installation/
[docs.reference.global-options#log_schema]: /docs/reference/global-options/#log_schema
[docs.sinks.elasticsearch]: /docs/reference/sinks/elasticsearch/
[docs.sources.socket]: /docs/reference/sources/socket/
[docs.transforms.regex_parser]: /docs/reference/transforms/regex_parser/
[docs.transforms]: /docs/reference/transforms/
