---
last_modified_on: "2020-07-14"
$schema: "/.meta/.schemas/highlights.json"
title: "Journald Unit Filtering Exclusions"
description: "The journald source can now exclude units."
author_github: "https://github.com/hoverbear"
hide_on_release_notes: false
pr_numbers: [2540]
release: "0.10.0"
tags: ["type: new feature", "source: journald"]
---

Often when you tap into the Journald source you're only really interested in a subset of the units, previously,
Vector supported this. However, sometimes you just want to exclude one or two.

Now, Vector can practice selective listening on Journald, ignoring units. Vector filters these directly at the
source, offering better performance and easier use.

You can find the old `units` option lives at `include_units` now, while a new `exclude_units` option now exists.

## Get Started

You can make the following changes in your `vector.toml` file:

```diff title="vector.toml"
  [sources.my_source_id]
    type = "journald" # required
    current_boot_only = true # optional, default
-    units = ["sshd", "ircd"]
+    exclude_units = ["zulip"] # optional, default
+    include_units = ["sshd", "ircd"] # optional, default
```


