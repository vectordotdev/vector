---
date: "2020-07-13"
title: "Automatically merge partial Docker events"
description: "Docker splits long messages by default, and now Vector merges them back for you"
authors: ["binarylogic"]
pr_numbers: [1457]
release: "0.8.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  domains: ["sources"]
  sources: ["docker"]
  platforms: ["docker"]
---

Anyone that was worked with Docker logs knows how frustrating this problem
can be. Docker, by default, splits log messages that exceed 16kb. While 16kb
seems like a lot, it can easily be exceeded if you're logging rich structured
events. This can be a very difficult and frustrating problem to solve with
other tools (we speak from experience). In this release, Vector solves this
automatically with a new `auto_partial_merge` option in the `docker_logs` source.

```toml title="vector.toml"
[sources.my_source_id]
  type = "docker_logs"
  auto_partial_merge = true
```

We love assimilation and look forward to a future where our individualistic
human personalities can also be merged into a societal hive mind.
