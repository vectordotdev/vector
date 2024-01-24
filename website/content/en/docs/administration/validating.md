---
title: Validating
weight: 5
tags: ["validate", "configuration"]
---

Vector provides a subcommand, [`validate`][validate], that checks the validity of your Vector configuration and exits.
Here's an example:

```bash
vector validate /etc/vector/vector.yaml
```

You can also check multiple files:

```bash
vector validate /etc/vector/vector*.toml
```

## How validation works

The [`validate`][validate] subcommand performs several sets of checks on the configuration you point
it to. If validation succeeds, Vector exits with a code of `0`; if it fails, it exits with a code of
`78`. At any time, you can see documentation for the command by running `vector validate --help`.

### Correctness checks

These checks verify the correctness of fields for [components] defined within all configuration
files, including:

1. That all of the [sources], [transforms], and [sinks] include all required fields.
2. All fields are of the proper type.

### Topology checks

These checks verify that the configuration file contains a valid topology:

1. At least one [source][sources] is defined
1. At least one [sink][sinks] is defined
1. All inputs for each topology component (specified using the `inputs` parameter) contain at least
  one value.
1. All inputs refer to valid and upstream [sources] or [transforms].

### Environment checks

Finally, these checks ensure that Vector is running in an environment that can support the
configured topology:

1. All components have the pre-requisites to run, e.g. data directories exist and are writable.
1. All sinks can connect to their specified targets.

These environment checks can be disabled using the [`--no-environment`][no_environment] flag:

```bash
vector validate --no-environment /etc/vector/vector.yaml
```

#### Skipping health checks

To validate the vector configuration even if the health-checked endpoints are not reachable
(for example, from a local workstation), but still run all the other environment checks, use
the [`--skip-healthchecks`][skip_healthchecks] flag:

```bash
vector validate --skip-healthchecks /etc/vector/vector.yaml
```

**Note:** The configured `data_dir` must still be writeable.

[components]: /components
[no_environment]: /docs/reference/cli/#validate-no-environment
[sinks]: /sinks
[sources]: /sources
[transforms]: /transforms
[validate]: /docs/reference/cli/#validate
