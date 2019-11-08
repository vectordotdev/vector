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
| `-t, --threads` | Limits the number of internal threads Vector can spawn. See the [Limiting Resources][docs.roles.agent#limiting-resources] in the [Agent role][docs.roles.agent] documentation. |
| `-v, --verbose` | Drops the log level to `debug`. |
| `-vv` | Drops the log level to `trace`, the lowest level possible. |

## Discovery

In order to help you explore its features Vector provides a subcommand `list`
that lists all available sources, transforms and sinks:

{% tabs %}
{% tab title="List" %}
```bash
vector list
```
{% endtab %}
{% endtabs %}

By default this prints a human readable representation of all components. You
can view options for customizing the output of `list` with `vector list --help`.

## Daemonizing

Vector does not _directly_ offer a way to daemonize the Vector process. We
highly recommend that you use a utility like [Systemd][urls.systemd] to
daemonize and manage your processes. Vector provides a
[`vector.service` file][urls.vector_systemd_file] for Systemd.

## Exit Codes

If Vector fails to start it will exit with one of the preferred exit codes
as defined by `sysexits.h`. A full list of exit codes can be found in the
[`exitcodes` Rust crate][urls.exit_codes]. The relevant codes that Vector uses
are:

| Code | Description |
|:-----|:------------|
| `0`  | No error. |
| `78` | Bad [configuration][docs.configuration]. |


[docs.configuration]: ../../usage/configuration
[docs.roles.agent#limiting-resources]: ../../setup/deployment/roles/agent.md#limiting-resources
[docs.roles.agent]: ../../setup/deployment/roles/agent.md
[docs.validating]: ../../usage/administration/validating.md
[urls.exit_codes]: https://docs.rs/exitcode/1.1.2/exitcode/#constants
[urls.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
[urls.vector_systemd_file]: https://github.com/timberio/vector/blob/master/distribution/systemd/vector.service
