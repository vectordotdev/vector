---
date: "2020-07-23"
title: "Custom DNS resolution removal"
description: "Vector once again follows the guidance of the host on DNS lookups."
authors: ["hoverbear"]
hide_on_release_notes: false
pr_numbers: [2812]
release: "0.10.0"
badges:
  type: "breaking change"
---

In Vector 0.10.0, we no longer support custom DNS servers. This feature was adding considerable code complexity and is better handled outside of Vector through tools like [`systemd-resolved`][urls.systemd_resolved].

In the interest of keeping Vector lean and understandable, as well as improving it's maintainability, we've chosen to remove it.

## Upgrade Guide

Make the following changes in your `vector.toml` file:

```diff title="vector.toml"
-  dns_servers = [...]
```

### Enabling DNS outside of Vector

If you were using this feature you may need to configure your host to consult DNS. This can be achieved through tools like [`systemd-resolved`][urls.systemd_resolved]. Alternatively, you can wrap Vector in a container and set the DNS for the container. This can be done via [`--dns` in `podman`/`docker`][urls.docker_dns] or

[urls.docker_dns]: https://docs.docker.com/config/containers/container-networking/#dns-services
[urls.systemd_resolved]: https://wiki.archlinux.org/index.php/Systemd-resolved
