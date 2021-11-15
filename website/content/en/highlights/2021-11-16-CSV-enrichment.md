2021-11-16-CSV-enrichment.md
---
date: "2021-11-15"
title: "CSV enrichment of data"
description: "A guide to using the new CSV enrichment feature"
authors: ["barieom", "lucperkins"]
pr_numbers: []
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: new feature
---

# Enrich your data with CSV 

We're excited to share that we've released a new feature that allows users to enrich events flowing through the topology using a CSV file. 

[Enrichment tables][] enables events to have more context and be more readable. This feature works by looking at a single row in a pointed CSV file, allowing users to map the data into the event using the full power of VRL. 

For example, when collecting events from IoT devices, you may want to keep your payloads coming from the devices to be small; by enriching events from a CSV file, users can reformat the data to be more human readable and provide better context (e.g., converting data emitted by the IoT device — `1`, `2`, `3` — to `"Low battery"`, `"Medium battery"`, `"High battery"`).


Let's stick with the IoT example from above, and let's assume that our CSV file contains the below:
```
code,message
1,"device battery full"
2,"device battery good"
3,"device battery ok"
4,"device battery low"
5,"device battery critical"
```

In order to use the csv file (let's call it `iot_remap.csv`), the following Vector configuration is required:
``` toml
[enrichment_tables.iot_remap]
type = "file"

[enrichment_tables.iot_remap.file]
path = "/etc/vector/iot_remap.csv"
encoding = { type = "csv" }

[enrichment_tables.iot_remap.schema]
iot_remap = "integer"
message = "string"
```

Now, to translate the output from IoT devices to human-readable messages in our `iot_remap.csv` file, the following is required. To do so, leverage the [`get_enrichment_table_record`][]:
```
[transforms.enrich_iot_logs]
type = "remap"
inputs = ["vector_agents"]
source = '''
. = parse_json!(.message)

code = del(.code)

row = get_enrichment_table_record!("codes", { "code": code })
.message = row.message
'''
```

For our next steps, we'll look to add encryption to this enrichment feature, but if you any feedback in the meantime, let us know on our [Discord chat][] or [Twitter][].


[Enrichment tables]:/docs/reference/glossary/#enrichment-tables
[`get_enrichment_table_record`]:/docs/reference/vrl/functions/#get_enrichment_table_record/
[Discord chat]:https://discord.com/invite/dX3bdkF
[Twitter]:https://twitter.com/vectordotdev