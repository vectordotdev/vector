---
date: "2020-04-14"
title: "Improved Multiline Support In The File Source"
description: "Merge multiple lines together based on rules"
authors: ["binarylogic"]
pr_numbers: [1852]
release: "0.8.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  domains: ["sources"]
  sources: ["file"]
---

One of the biggest frustrations we've heard from users in this space is the
inability to merge lines together. Such a simple task can be incredibly
complex and hard. Fear not! We plan to add first-class support for solving
this problem.

In addition to the recently added [automatic merging of Docker
logs][docs.sources.docker_logs#auto_partial_merge], we also added [better multi-line
support][docs.sources.file#multiline] to our [`file` source][docs.sources.file].
These options are very expressive and should solve the vast majority of
multi-line merging problems.

For example. Given the following lines:

```text
foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)
  from foobar.rb:6:in `bar'
  from foobar.rb:2:in `foo'
  from foobar.rb:9:in `<main>'
```

You can merge them with the following config:

```toml title="vector.toml"
[sources.my_file_source]
  type = "file"
  # ...

  [sources.my_file_source.multiline]
    start_pattern = "^[^\\s]"
    mode = "continue_through"
    condition_pattern = "^[\\s]+from"
    timeout_ms = 1000
```

And if this doesn't do it, you can always fall back to the [`lua` transform][docs.transforms.lua].

[docs.sources.docker_logs#auto_partial_merge]: /docs/reference/configuration/sources/docker_logs/#auto_partial_merge
[docs.sources.file#multiline]: /docs/reference/configuration/sources/file/#multiline
[docs.sources.file]: /docs/reference/configuration/sources/file/
[docs.transforms.lua]: /docs/reference/configuration/transforms/lua/
