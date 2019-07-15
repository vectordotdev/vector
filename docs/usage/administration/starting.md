---
description: Starting Vector
---

# Starting

Vector can be started by calling the `vector` binary directly, no command is necessary.

{% tabs %}
{% tab title="Manual" %}
```bash
vector --config /etc/vector/vector.toml
```
{% endtab %}
{% tab title="Systemd" %}
```bash
sudo systemctl start vector
```
{% endtab %}
{% tab title="Initd" %}
```bash
/etc/init.d/vector start
```
{% endtab %}
{% tab title="Homebrew" %}
```bash
brew services start vector
```
{% endtab %}
{% endtabs %}

## Flags

| Flag | Description |
| :--- | :--- |
| **Required** |  |  |
| `-c, --config <path>` | Path the Vector [configuration file][docs.configuration]. |
| **Optional** |  |  |
| `-d, --dry-run` | Vector will [validate configuration][docs.validating] and exit. | 
| `-q, --quiet` | Raises the log level to `warn`. |
| `-qq` | Raises the log level to `error`, the highest level possible. |
| `-r, --require-healthy` | Causes vector to immediately exit if any sinks fail their healthchecks. |
| `-t, --threads` | Limits the number of internal threads Vector can spawn. See the [Limiting Resources][docs.agent_role.limiting-resources] in the [Agent role][docs.agent_role] documentation. |
| `-v, --verbose` | Drops the log level to `debug`. |
| `-vv` | Drops the log level to `trace`, the lowest level possible. |

## Daemonizing

Vector does not _directly_ offer a way to daemonize the Vector process. We
highly recommend that you use a utility like [Systemd][url.systemd] to
daemonize and manage your processes. Vector provides a
[`vector.service` file][url.vector_systemd_file] for Systemd.


[docs.agent_role.limiting-resources]: ../../setup/deployment/roles/agent.md#limiting-resources
[docs.agent_role]: ../../setup/deployment/roles/agent.md
[docs.configuration]: ../../usage/configuration
[docs.validating]: ../../usage/administration/validating.md
[url.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
[url.vector_systemd_file]: https://github.com/timberio/vector/blob/master/distribution/systemd/vector.service
