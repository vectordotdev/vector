---
title: Validating
description: Validate Vector's configuration
---

Vector provides a `--dry-run` option to validate configuration only:

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
4. All `inputs` values reference valid and upstream [source][docs.sources] or [transform][docs.transforms] components. See [composition][docs.configuration#composition] for more info.
5. All [sources][docs.sources], [tranforms][docs.transforms], and [sinks][docs.sinks] include required options.
6. All options are of the proper [type][docs.configuration#value-types].
7. All [sink][docs.sinks] health check if the `--require-healthy` option is supplied.


[docs.configuration#composition]: /docs/setup/configuration#composition
[docs.configuration#value-types]: /docs/setup/configuration#value-types
[docs.sinks]: /docs/components/sinks
[docs.sources]: /docs/components/sources
[docs.transforms]: /docs/components/transforms
