---
title: Validating
weight: 5
tags: ["validate", "configuration"]
---

Vector provides a subcommand, [`validate`][validate], that checks the validity of any number of configuration files and
then exits. Here's an example usage:

```bash
vector validate /etc/vector/vector.toml
```

You can also check multiple files



[validate]: /docs/reference/cli/#validate



{{% tabs default="All checks" %}}
{{% tab title="All checks" %}}

```bash
vector validate /etc/vector/vector.toml
```

The `validate` subcommand checks the correctness of fields for components defined within a configuration file, including:

1. That [sources](/docs/reference/configuration/sources), [transforms](/docs/reference/configuration/transforms), and [sinks](/docs/reference/configuration/sinks) include all non-optional fields.
2. All fields are of the proper type.

The following group of checks verifies that the configuration file contains a valid topology, and can be disabled with flags such as `--no-topolog`y, expanding the above checks with the following:

3. At least one source is defined.
4. At least one sink is defined.
5. All `inputs` contain at least one value (cannot be empty).
6. All `inputs` refer to valid and upstream source or transform components.

The following group of checks require the runtime environment to pass successfully, and can be disabled with flags such as `--no-environment`, expanding the above checks with the following:

7. All components are capable of running, for example that data directories exist and are writable, etc.
8. All sinks are able to connect to their targets.

If validation fails, Vector exits with a code of `78`; if validation succeeds, Vector exits with a code of `0`.

To see other customization options for the validate subcommand run vector validate `--help`.

{{% /tab %}}
{{% tab title="Config only" %}}

```bash
vector validate --no-environment --no-topology /etc/vector/*.toml
```

The `validate` subcommand checks the correctness of fields for components defined within a configuration file, including:

1. That sources, transforms, and sinks include all non-optional fields.
2. All fields are of the proper type.

The following group of checks verifies that the configuration file contains a valid topology and can be disabled with flags such as `--no-topology`, expanding the above checks with the following:

3. At least one source is defined.
4. At least one sink is defined.
5. All `inputs` contain at least one value (cannot be empty).
6. All `inputs` refer to valid and upstream source or transform components.

The following group of checks require the runtime environment to pass successfully, and can be disabled with flags such as `--no-environment`, expanding the above checks with the following:

7. All components can run, for example that data directories exist and are writable, etc.
8. All sinks can connect to their targets.

If validation fails, Vector exits with a code of `78`; if validation succeeds, Vector exists with a code of `0`.

To see other customization options for the `validate` subcommand run `vector validate --help`.
{{% /tab %}}
{{% /tabs %}}
