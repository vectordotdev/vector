---
last_modified_on: "2020-07-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Removal custom DNS resolution"
description: "Vector once again follows the guidance of the host on DNS lookups."
author_github: "https://github.com/hoverbear"
hide_on_release_notes: false
pr_numbers: [2812]
release: "0.10.0"
tags: ["type: breaking change"]
---

In Vector 0.10.0, we no longer support custom DNS servers. This feature was adding considerable code complexity and we found the feature to be relatively unused based on our survey of users and feedback.

In the interest of keeping Vector lean and understandable, as well as improving it's maintainability, we've chosen to remove it.

## Upgrade Guide

Make the following changes in your `vector.toml` file:

```diff title="vector.toml"
-  dns_servers = [...]
```

If you were using this feature with a custom DNS you may need to configure your host to consult this DNS, or wrap Vector in a container and set the DNS for the container. This can be done via [`--dns` in `podman`/`docker`][urls.docker_dns].

[urls.docker_dns]: https://docs.docker.com/config/containers/container-networking/#dns-services
