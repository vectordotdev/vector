---
description: Tuning Vector
---

# Tuning

Vector is written in [Rust][url.rust] and therefore does not inclue a runtime
or VM. There are no special service level steps you need to do to improve
performance. By default, Vector will take full advantage of all system
resources.

Conversely, when deploying Vector in the [agent role][docs.agent_role] you'll
typically want to limit resources. This is covered in detail in the
[Agent role system configuration][docs.agent_role.system-configuration] section.


[docs.agent_role.system-configuration]: ../../setup/deployment/roles/agent.md#system-configuration
[docs.agent_role]: ../../setup/deployment/roles/agent.md
[url.rust]: https://www.rust-lang.org/
