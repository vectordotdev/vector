---
date: "2020-07-13"
title: "Set the Lua transform `search_dirs` option to Vector's config dir by default"
short: Default `search_dirs` for Lua
description: "This allows you to place Lua scripts in the same dir as your Vector config"
authors: ["binarylogic"]
hide_on_release_notes: true
pr_numbers: [2274]
release: "0.9.0"
badges:
  type: "breaking change"
  domains: ["transforms"]
  transforms: ["lua"]
---

As part of our recent Lua improvements we've defaulted the `search_dirs` option
to the same directory as your Vector configuration file(s). This is usually
what's expected and allows you to place all of your Vector related files
together.

## Upgrade Guide

Make the following changes in your `vector.toml` file if your Lua files are not
in the same directory as your Vector configuration file:

```diff title="vector.toml"
[transform.my-script]
   type = "lua"
+  search_dirs = "/my/other/dir"
```

That's it!
