---
last_modified_on: "2020-03-31"
$schema: "/.meta/.schemas/highlights.json"
title: "Use Custom DNS Servers"
description: "Point Vector to custom DNS servers"
author_github: "https://github.com/binarylogic"
pr_numbers: [1118, 1362, 1371, 1400, 1451]
release: "0.6.0"
hide_on_release_notes: false
tags: ["type: new feature", "domain: networking"]
---

We're modern progressive parents and aren't about to tell Vector who it can and
can't hang out with. As such, we're now allowing you to specify custom DNS
servers in your configs.

<!--truncate-->

The configuration isn't complicated, it's a global array field `dns_servers`:

```toml
dns_servers = ["0.0.0.0:53"]
```

When `dns_servers` is set Vector will ignore the system configuration and use
only the list of DNS servers provided.



