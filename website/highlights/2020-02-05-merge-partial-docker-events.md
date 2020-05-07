---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Automatically merge partial Docker events"
description: "Docker splits long messages by default, and now Vector merges them back for you"
author_github: "https://github.com/binarylogic"
pr_numbers: [1457]
release: "0.8.0"
hide_on_release_notes: false
tags: ["type: enhancement", "domain: sources", "source: docker", "platform: docker"]
---

Anyone that was worked with Docker logs knows how frustrating this problem
can be. Docker, by default, splits log messages that exceed 16kb. While 16kb
seems like a lot, it can easily be exceeded if you're logging rich structured
events. This can be a very difficult and frustrating problem to solve with
other tools (we speak from experience). In this release Vector solves this
automatically with a new `auto_partial_merge` option in the `docker` source.

```toml title="vector.toml"
[sources.my_source_id]
  type = "docker"
  auto_partial_merge = true
```

We love assimilation and look forward to a future where our individualistic
human personalities can also be merged into a societal hive mind.



