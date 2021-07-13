---
date: "2020-07-13"
title: "RegexSet support to `regex` transform"
description: "Efficiently run multiple regexes in the same transform!"
authors: ["hoverbear"]
hide_on_release_notes: false
pr_numbers: [2493]
release: "0.10.0"
badges:
  type: "enhancement"
  domains: ["transforms"]
  transforms: ["regex_parser"]
---

Contributor [Mattias Endler (@mre)][urls.endler_dev] taught the [`regex_parser` transform][urls.vector_regex_parser] how to handle multiple regexes at a time efficiently!

## Get Started

Make the following changes in your `vector.toml` file:

In order to avoid a **deprecation warning** you should update any `regex_parser` components to use the new syntax:

```diff title="vector.toml"
 [transforms.example]
   type = "regex_parser"
-  regex = "..."
+  patterns = [
+    "...",
+    # Any new regexes you might want!
+  ]
```

You should also review your pipelines for instances where you have a `regex_parser -> [... ->] regex_parser` step, you may be able to collapse these now and shave a few a nanoseconds off your events. 😉

[urls.endler_dev]: https://endler.dev/
[urls.vector_regex_parser]: /docs/reference/vrl/functions/#parse_regex
