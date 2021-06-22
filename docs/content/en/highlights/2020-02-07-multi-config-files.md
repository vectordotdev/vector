---
date: "2020-04-13"
title: "Support For Multiple Config Files"
description: "A better way to manage complex Vector configurations"
authors: ["binarylogic"]
pr_numbers: [1725]
release: "0.8.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["config"]
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
