---
title: Validating
description: Validate Vector's configuration
---

# Validating

Vector provides a subcommand `validate` which checks the validity of a
configuration file and then exits:

{% code-tabs %}
{% code-tabs-item title="fields only" %}
```bash
vector validate --config /etc/vector/vector.toml
```
{% endcode-tabs-item %}
{% code-tabs-item title="fields + topology" %}
```bash
vector validate --config /etc/vector/vector.toml --topology
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The validate subcommand checks the correctness of fields for components defined
within a configuration file, including:

1. All [sources][docs.sources], [transforms][docs.transforms], and
[sinks][docs.sinks] include all non-optional fields.
2. All fields are of the proper [type][docs.configuration#value-types].

If validation fails, Vector will exit with a `78`, and if validation succeeds
Vector will exit with a `0`.

These checks can be expanded with flags such as `--topology`, which causes
`validate` to also verify that the configuration file contains a valid topology,
expanding the above checks with the following:

3. At least one [source][docs.sources] is defined.
4. At least one [sink][docs.sinks] is defined.
5. All `inputs` values contain at least one value (cannot be empty).
6. All `inputs` values reference valid and upstream [source][docs.sources] or
[transform][docs.transforms] components. See
[composition][docs.configuration#composition] for more info.

To see other customization options for the `validate` subcommand run
`vector validate --help`.

## Validating Environment

Vector also provides a `--dry-run` option which prevents regular execution and
instead validates a configuration file as well as the runtime environment:

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="manual"
  values={[
    { label: 'Config Only', value: 'config', },
    { label: 'Config + Healthchecks', value: 'config_healthchecks', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="config">

```bash
vector --config /etc/vector/vector.toml --dry-run
```

</TabItem>
<TabItem value="config_healthchecks">

```bash
vector --config /etc/vector/vector.toml --dry-run --require-healthy
```

</TabItem>
</Tabs>

If a dry run fails, Vector will exit with a `78`, and if it succeeds Vector
will exit with a `0`.

A dry run expands upon the `validation` checks above with the following:

7. All components are capable of running (data directories exist, are writable,
etc).

You'll notice in the second example above you can pass the `--require-healthy`
flag to also run health checks for all defined sinks.

8. All [sinks][docs.sinks] are able to connect to their targets.


[docs.configuration#composition]: /docs/setup/configuration#composition
[docs.configuration#value-types]: /docs/setup/configuration#value-types
[docs.sinks]: /docs/components/sinks
[docs.sources]: /docs/components/sources
[docs.transforms]: /docs/components/transforms
