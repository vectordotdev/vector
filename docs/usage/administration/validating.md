---
description: Validate Vector's configuration
---

# Validating

Vector provides a `--dry-run` option to validate configuration only:

{% code-tabs %}
{% code-tabs-item title="config only" %}
```bash
vector --config /etc/vector/vector.toml --dry-run
```
{% endcode-tabs-item %}
{% code-tabs-item title="config + health checks" %}
```bash
vector --config /etc/vector/vector.toml --dry-run --require-healthy
```
{% endcode-tabs-item %}
{% endcode-tabs %}

If validation fails, Vector will exit with a `78`, and if validation succeeds
Vector will exit with a `0`.

You'll notice in the second example above you can pass the `--require-healthy`
flag to also run health checks for all defined sinks.

This operation is useful to validate configuration changes before going live.

## Checks

For clarify, Vector validates the following:

1. At least one [source][docs.sources] is defined.
2. At least one [sink][docs.sinks] is defined.
3. The all `inputs` values contain at least one value (cannot be empty).
4. All `inputs` values reference valid and upstream [source][docs.sources] or [transform][docs.transforms] components. See [composition][docs.configuration.composition] for more info.
5. All [sources][docs.sources], [tranforms][docs.transforms], and [sinks][docs.sinks] include required options.
6. All options are of the proper [type][docs.configuration.types].
7. All [sink][docs.sinks] health check if the `--require-healthy` option is supplied.


[docs.configuration.composition]: ../../usage/configuration/README.md#composition
[docs.configuration.types]: ../../usage/configuration/README.md#types
[docs.sinks]: ../../usage/configuration/sinks/README.md
[docs.sources]: ../../usage/configuration/sources/README.md
[docs.transforms]: ../../usage/configuration/transforms/README.md
