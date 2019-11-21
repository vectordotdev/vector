---
title: Tuning
description: Tuning Vector
---

Vector is written in [Rust][urls.rust] and therefore does not include a runtime
or VM. There are no special service level steps you need to do to improve
performance. By default, Vector will take full advantage of all system
resources.

Conversely, when deploying Vector in the [agent role][docs.roles.agent] you'll
typically want to limit resources. This is covered in detail in the
[Agent role system configuration][docs.roles.agent#system-configuration] section.


[docs.roles.agent#system-configuration]: /docs/setup/deployment/roles/agent#system-configuration
[docs.roles.agent]: /docs/setup/deployment/roles/agent
[urls.rust]: https://www.rust-lang.org/
