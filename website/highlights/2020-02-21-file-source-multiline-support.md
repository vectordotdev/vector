---
last_modified_on: "2020-04-14"
$schema: "/.meta/.schemas/highlights.json"
title: "Improved Multiline Support In The File Source"
description: "Merge multiple lines together based on rules"
author_github: "https://github.com/binarylogic"
pr_numbers: [1852]
release: "0.8.0"
hide_on_release_notes: false
tags: ["type: enhancement", "domain: sources", "source: file"]
---

One of the biggest frustrations we've heard from users in this space is the
inability to merge lines together. Such a simple task can be incredibly
complex and hard. Fear not! We plan to add first-class support for solving
this problem.

In addition to the recently added [automatic merging of Docker
logs][docs.sources.docker#auto_partial_merge], we also added [better multiline
[support][docs.sources.file#multiline] to our [`file` source][docs.sources.file].
These options are very expressive and should solve the vast majority of
multiline merging problems.

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

And if this doesn't do it, you can always fallback
to our [`lua` transform][docs.transforms.lua].


[docs.sources.docker#auto_partial_merge]: /docs/reference/sources/docker/#auto_partial_merge
[docs.sources.file#multiline]: /docs/reference/sources/file/#multiline
[docs.sources.file]: /docs/reference/sources/file/
[docs.transforms.lua]: /docs/reference/transforms/lua/
