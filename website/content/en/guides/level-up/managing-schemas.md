---
title: Encoding, Decoding, and Managing Schemas
short: Schemas
description: Learn how to manage log schemas with Vector.
author_github: https://github.com/hoverbear
domain: schemas
weight: 2
tags: ["schemas", "schema management", "logs", "level up", "guides", "guide"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand the [basic Vector concepts][docs.about.concepts]
* You understand [how to set up a basic pipeline][docs.setup.quickstart].

[docs.about.concepts]: /docs/about/concepts/
[docs.setup.quickstart]: /docs/setup/quickstart/
{{< /requirement >}}

Data comes in all shapes and sizes. Vector has an array (let's call it a vector 😎) of composable functionality for
decoding your events in the right format, transforming them into the right shape, and passing that data on downstream.

While your first uses of Vector might be connecting `stdin` to `honeycomb`, eventually you're going to have other
requirements. Maybe regional laws prevent you from shipping certain data, or you need to do some data munging to conform
some logs to the rest of your system. With a little configuration we can teach Vector to solve all these problems!

## Overriding Global Field Names

By default, Vector primarily operates on three fields: `host`, `message`, and `timestamp`.

```json
{
  "host": "my.host.com",
  "message": "some important content",
  "timestamp": "2019-11-01T21:15:47+00:00"
}
```

Vector sets these fields on logs as it ingests data (from a [source][docs.sources]). It may be that your data does not
follow this convention. In this case you can modify the global defaults for all incoming data in the `log_schema`
section of your `vector.toml`.

```toml title="vector.toml"
[log_schema]
host_key = "instance" # default "host"
message_key = "info" # default "message"
timestamp_key = "datetime" # default "timestamp"

# Sources, transforms, and sinks...
```

{{< warning >}}
Not all sources use the `host` field.
{{< /warning >}}

We find this feature is useful when used with simple configs! As your number of components grows, your needs will change
and you'll likely need to configure this at a more fine-grained level.


### Example: Custom timestamp field

Some services will produce logs with the timestamp field mapped to `@timestamp` or some other value.

If your vector pipeline is only working with data passing through these systems, you can add the following to your
`vector.toml`:

```toml title="vector.toml"
[log_schema]
  timestamp_key = "@timestamp"  # Applies to all sources, sinks, and transforms!

[sources.my_naming_confused_source]
  type = "logplex"
  address = "0.0.0.0:8088"
```


## Pipeline field filtering

Sometimes it is advantageous to filter out specific fields during the pipeline. You can use a `remap` transform
to do this.

Commonly you'll want to do this near either the source or sink of your pipeline. Some example use cases:

* Dropping `email`, `passport_number`, or other personally identifiable information from logs before distributing them
  to third party services.
* Filtering data for compliance with the GDPR or other regional laws. (eg EU to US dataflows)
* Reducing the volume of data on a particular endpoint.

A transform of this type looks like this:

```toml title="vector.toml"
[transforms.strip_personal_details]
type = "remap"
inputs = ["my-source-id"]
source = '''
  del(.email, .passport_number)
'''
```

The `remap` transform has a wealth of mapping functions, and in cases where we wish to flip this concept and drop all fields except for a list of exceptions we can do that with the `only_fields` function:

```toml title="vector.toml"
[transforms.strip_personal_details]
type = "remap"
inputs = ["my-source-id"]
source = '''
  only_fields(.timestamp, .message, .host, .user_id)
'''
```

### Example: Filtering data for GDPR compliance

Let's pretend we have a nice well behaved application piping Vector logs like the following:

```json
{ "id": "user1", "gdpr": false, "email": "us-user1@datadoghq.com" }
{ "id": "user2", "gdpr": false, "email": "us-user2@datadoghq.com" }
{ "id": "user3", "gdpr": true, "email": "eu-user3@datadoghq.com" }
```

In our theoretical product, we're expanding into the EU and want to comply with the GDPR. In our case, that means our
application can't send EU user data to our US based kafka. (We're not lawyers, this is not a magic GDPR-compliance
config, just a little example!)

We can build a config that will do the first part of this, but we'll just output to console for ease of this example.

```toml title="vector.toml"
data_dir = "./data"
dns_servers = []

[sources.application]
max_length = 102400
type = "stdin"

[transforms.parse]
inputs = ["application"]
type = "remap"
source = '''
. = parse_json!(.message)
'''

[transforms.not_gdpr]
type = "filter"
inputs = ["parse"]
condition = ".gdpr == false"

[transforms.gdpr_to_strip]
type = "filter"
inputs = ["parse"]
condition = ".gdpr == true"

[transforms.gdpr_stripped]
type = "remap"
inputs = ["gdpr_to_strip"]
source = "del(.email)"

[sinks.console]
healthcheck = true
inputs = ["not_gdpr", "gdpr_stripped"]
type = "console"
encoding.codec = "json"
[sinks.console.buffer]
type = "memory"
max_events = 500
when_full = "block"
```

Let's have a look:

```bash
$ cat <<-EOF | cargo run -- --config test.toml
{ "id": "user1", "gdpr": false, "email": "us-user1@datadoghq.com" }
{ "id": "user2", "gdpr": false, "email": "us-user2@datadoghq.com" }
{ "id": "user3", "gdpr": true, "email": "eu-user3@datadoghq.com" }
EOF
Feb 05 16:13:59.241  INFO source{name=application type=stdin}: vector::sources::stdin: finished sending
{"id":"user1","timestamp":"2020-02-06T00:13:59.241801798Z","host":"obsidian","email":"us-user1@datadoghq.com","gdpr":false}
{"gdpr":false,"host":"obsidian","email":"us-user2@datadoghq.com","timestamp":"2020-02-06T00:13:59.241815255Z","id":"user2"}
{"id":"user3","gdpr":true,"host":"obsidian","timestamp":"2020-02-06T00:13:59.241816010Z"}
Feb 05 16:15:27.945  INFO vector: Shutting down.
```

Don't know where events are coming from? You can do a geo ip lookup using VRL [enrichment functions][docs.enrichment_functions] to transform an `ipv4` field and get a grip on that!


## Sink field filtering

While it's often reasonable to remove this kind of data at the pipeline level, we identified use cases that involve
using values in sinks from these fields in sink configuration.

The applications for this include some of the reasons discussed in
[Pipeline field filtering](#pipeline-field-filtering), but also:

* Stripping off routing related fields
* Ensuring a specific sink will only ever output specific fields (or never output certain fields)


### Example: Per host kafka topics

Lets take a look at what that might look like:

```toml title="vector.toml"
[sinks.output]
  inputs = ["demo_logs"]
  type = "kafka"

  # Put events in the host specific topic.
  topic = "{{service}}"
  encoding.except_fields = ["service"] # Remove this field now and save some bytes
  # ...
```

{{< warning >}}
Beware: not all fields are templatable! Make sure to check the documentation and test before deploying. If you find
a field which you want templatable open an issue and let us know.
{{< /warning >}}

## Moving and Concatenating Fields

It's fairly common for one part of your pipeline to expect a field to be named differently than another part! The `remap` transform can also be used to slide data around for you.

```toml title="vector.toml"
[transforms.rename_timestamp]
  type = "remap"
  inputs = ["source0"]
  source = '''
    ."@timestamp" = .timestamp
    del(.timestamp)
  '''
```

Other times you might need to concatenate fields together, or perform arithmetic on their numerical values, the [`remap`][docs.transforms.remap] transform can be used to do all of these things.

It's useful for when:

* You need to adapt or reshape data to fit into possibly older or newer systems.
* You need to concatenate `first_name` and `last_name` into a `name` field. (Suppose they didn't read
  [Falsehoods about names](https://www.kalzumeus.com/2010/06/17/falsehoods-programmers-believe-about-names/)).


### Example: Mooshing together name fields

Let's pretend one of your teammates falsely assumed folks always have first and last names, so we have a `first_name`
and a `last_name` field coming from a source, and we'd like to output a `name` field to a sink.

```toml title="vector.toml"
[transforms.moosh_names]
  type = "remap"
  inputs = ["source0"]
  source = '''
    .name = .first_name + " " + .last_name
    del(.first_name, .last_name)
  '''
```

## Coercing Data Types

Occasionally services will provide you with data that is in the right shape, but the types are wrong. Perhaps a string
should be a number, or vice versa.

Gadzooks! The [`remap`][docs.transforms.remap] transform is also the correct tool for this job!

```toml title="vector.toml"
[transforms.correct_source_types]
  type = "remap"
  inputs = ["source0"]
  source = '''
    .count = int(.count)
    .date = timestamp(.date, "%F")
  '''
```

Remember that you can follow the coercion mappings with `del` or `only_fields` functions, empowering it to drop
fields you've not specified. Coercer? More like enforcer.

### Example: Coercing into a specific format

There are a lot of ways to represent time. In the US folks tend to use `MM/DD/YYYY` or the (more reasonable)
`YYYY/MM/DD` which Canada and China like. In the EU, South America, and Africa they prefer `DD/MM/YYYY`. Like personal
identities, all are valid. Vector lets us take in timestamps and output specific formats easily.

To do this we'll use the `timestamp` function with a format string argument. To build a format string, we can reference the
[`strftime`](https://docs.rs/chrono/0.4.10/chrono/format/strftime/index.html) documentation. Let's ship some Canadian
friendly logs up to the great white north!

```toml title="vector.toml"
[transforms.format_timestamp]
  type = "remap"
  source = '''
    .timestamp = timestamp(.timestamp, "%Y/%m/%d:%H:%M:%S %z")
  '''
```

## Working with data formats

Not all logs come structured. Some services provide JSON, some provide plaintext, others ship around protobufs. With
Vector you can handle them all.

Generally Vector will be able to determine the encodings to use by the source or sink used. In some cases, multiple are
supported. In these cases, you can use the `encoding` option.

The [`console`][docs.sinks.console] sink supports both `json` and `text` as its
output format

```toml title="vector.toml"
[sinks.print]
  type = "console"
  inputs = ["source0"]
  target = "stdout"
  encoding.codec = "json"
```

You can also use a transform like [`remap`][docs.transforms.remap] or to parse out
data in a given field.

## Parting thoughts

Exploring this article, we can see that Vector is able to consume multiple (even non-standard) formats of logs. We saw
that Vector can then reshape the data according to your needs. Then Vector can pass this data along.

Let's consider some novel uses for Vector, given these tools! Vector can work as:

* A sanitization tool, ensuring malformed events never reach a service.
* A privacy tool, removing sensitive data before it leaves your infrastructure.
* A data corrector, adapting legacy systems to more modern systems which have evolved.

Where are you deploying Vector? Let us know, maybe we can help optimize it!

[docs.sinks.console]: /docs/reference/configuration/sinks/console/
[docs.sources]: /docs/reference/configuration/sources/
[docs.transforms.remap]: /docs/reference/configuration/transforms/remap/
[docs.enrichment_functions]: /docs/reference/vrl/functions/#enrichment-functions

