---
last_modified_on: "2020-03-31"
id: custom-dns
title: "Use Custom DNS Servers"
description: "Point Vector to custom DNS servers"
author_github: https://github.com/Jeffail
tags: ["type: announcement", "domain: networking"]
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



