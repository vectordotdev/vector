---
last_modified_on: "2020-07-14"
$schema: "/.meta/.schemas/highlights.json"
title: "Journald Unit Filtering"
description: "The journald source can now filter messages before they enter your pipeline."
author_github: "https://github.com/hoverbear"
hide_on_release_notes: false
pr_numbers: [2540]
release: "0.10.0"
tags: ["type: new feature"]
---

Often when you tap into the Journald source you're only really interested in a subset of the units.

Vector can now filter those directly at the source, offering better performance and easier use.

## Get Started

Consider if you should make the following changes in your `vector.toml` file:

```diff title="vector.toml"
  [sources.my_source_id]
    type = "journald" # required
    current_boot_only = true # optional, default
+    exclude_units = ["zulip"] # optional, default
     # or ...
+    include_units = ["sshd", "ircd"] # optional, default
```


