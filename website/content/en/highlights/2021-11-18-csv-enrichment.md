---
date: "2021-11-18"
title: "Enrich your observability data from a CSV"
description: "A guide to using the new CSV enrichment feature"
authors: ["barieom", "lucperkins"]
pr_numbers: [9069]
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: new feature
---

We're excited to share that we've released a new feature that enables users to
enrich events flowing through the topology using a CSV file.

[Enrichment tables] are a new concept in Vector that enables you to enrich
events from external data sources. To start, we've added the ability to enrich
events from a CSV file by looking up a row, or rows, matching provided
conditions, allowing users to map the data into the event using the full power
of VRL.

To support mapping events based on enrichment table data, two new VRL functions
are now available:

- [`get_enrichment_table_record`][get_enrichment_table_record] works by looking
  up a single row CSV file
- [`find_enrichment_table_records`][find_enrichment_table_records] can return
  multiple rows in an array format for more complex use cases

For example, when collecting events from IoT devices, you may want to keep your
payloads coming from the devices to be small. By enriching events from a CSV
file, users can reformat the data to be more human readable and provide better
context (e.g., converting data emitted by the IoT device — `1`, `2`, `3` — to
`"Low battery"`, `"Medium battery"`, `"High battery"`).

Let's stick with the IoT example from above, and let's assume that our CSV file
contains the below:

```csv
code,message
1,"device battery full"
2,"device battery good"
3,"device battery ok"
4,"device battery low"
5,"device battery critical"
```

In order to use the csv file (let's call it `iot_remap.csv`), the following
Vector configuration is required:

``` toml
[enrichment_tables.iot_remap]
type = "file"

[enrichment_tables.iot_remap.file]
path = "/etc/vector/iot_remap.csv"
encoding = { type = "csv" }

[enrichment_tables.iot_remap.schema]
code = "integer"
message = "string"
```

Now, to translate the output from IoT devices to human-readable messages in our
`iot_remap.csv` we can make use of the
[`get_enrichment_table_record`][get_enrichment_table_record] function:

``` toml
[transforms.enrich_iot_logs]
type = "remap"
inputs = ["vector_agents"]
source = '''
. = parse_json!(.message)

code = del(.code)

row = get_enrichment_table_record!("iot_remap", { "code":  code })
.message = row.message
'''
```

For our next steps, we'll look to add support for `or` conditions and add
additional enrichment table types (e.g., reading from Redis), but if you any
feedback in the meantime, let us know on our [Discord chat] or [Twitter].

[Enrichment tables]: /docs/reference/glossary/#enrichment-tables
[get_enrichment_table_record]: /docs/reference/vrl/functions/#get_enrichment_table_record
[find_enrichment_table_records]: /docs/reference/vrl/functions/#find_enrichment_table_records
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
