---
date: "2021-01-31"
title: "Improved `file` source checkpointing"
description: "The `file` source checkpointing strategy has been improved to solve surprising edge cases."
authors: ["binarylogic"]
featured: false
pr_numbers: [6178]
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  domains: ["sources"]
  sources: ["file"]
---

The Vector [`file` source][file_source] included an option called `start_at_beginning` that would determine where
Vector would begin reading a file based on a [variety of conditions][conditions]. As you can see, these conditions were
quite confusing. To resolve this, [PR 6178][pr_6178] deprecated the `start_at_beginning` option and replaced it with new
[`ignore_checkpoints`][ignore_checkpoints] and [`read_from`][read_from] options. Migrating is easy:

```diff
 [sources.file]
 type = "file"
-start_at_beginning = true
+ignore_checkpoints = false # default
+read_from = "beginning" # default
```

Adjust as necessary. The above values are the defaults and are not required to be specified.

[conditions]: https://github.com/vectordotdev/vector/issues/1020
[file_source]: /docs/reference/configuration/sources/file/
[ignore_checkpoints]: /docs/reference/configuration/sources/file/#ignore_checkpoints
[pr_6178]: https://github.com/vectordotdev/vector/pull/6178
[read_from]: /docs/reference/configuration/sources/file/#read_from
