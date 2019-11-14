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

## Configuration

In order to assist with writing a Vector configuration file it's possible to
generate one containing a custom set of components with the subcommand
`generate`:

{% tabs %}
{% tab title="List" %}
```bash
vector generate 'stdin|json_parser,add_fields|console'
```
{% endtab %}
{% endtabs %}

The format of a generate expression is three comma-separated lists of sources,
transforms and sinks respectively, separated by pipes. If subsequent component
types are not needed then their pipes can be omitted from the expression.

Here are some examples:

- `|json_parser` prints a `json_parser` transform.
- `||file,http` prints a `file` and `http` sink.
- `stdin||http` prints a `stdin` source and an `http` sink.

Vector makes a best attempt at constructing a sensible topology. The first
transform generated will consume from all sources and subsequent transforms
will consume from their predecessor. All sinks will consume from the last
transform or, if none are specified, from all sources. It is then up to you to
restructure the `inputs` of each component to build the topology you need.

Generated components are given incremental names (`source1`, `source2`, etc)
which should be replaced in order to provide better context.

You can view options for customizing the output of `generate` with
`vector generate --help`.

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
