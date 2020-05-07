---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Support For Multiple Config Files"
description: "A better way to manage complex Vector configurations"
author_github: "https://github.com/binarylogic"
pr_numbers: [1725]
release: "0.8.0"
hide_on_release_notes: false
tags: ["type: new feature", "domain: config"]
---

Vector is now able to execute a topology spread across multiple config files,
which allows you to break large pipelines down into bite size, easier managed,
chunks. Running them is as simple as:

```bash
vector -c ./configs/first.toml -c ./configs/second.toml -c ./more/*.toml
```

[Subscribe to our newsletter][pages.community] and you'll be notified when we
learn how to do this with human emotions.


[pages.community]: /community/
