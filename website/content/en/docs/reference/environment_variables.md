---
title: Environment Variables
short: Environment Variables
weight: 5
tags: ["env", "environment variables", "interpolation"]
---

Vector interpolates environment variables within your configuration file with
the following syntax:

```yaml
transforms:
  add_host:
    type: "remap"
    source: |
      # Basic usage. "$HOSTNAME" also works.
      .host = "${HOSTNAME}" # or "$HOSTNAME"

      # Setting a default value when not present.
      .environment = "${ENV:-development}"

      # Requiring an environment variable to be present.
      .tenant = "${TENANT:?tenant must be supplied}"
```

## Default values

Default values can be supplied using `:-` syntax:

```yaml
option: "${ENV_VAR:-default}" # default value if variable is unset or empty
```

Or the `-` syntax:

```yaml
option: "${ENV_VAR-default}" # default value only if variable is unset
```

## Required variables

Environment variables that are required can be specified using `:?` syntax:

```yaml
option: "${ENV_VAR:?err}" # Vector exits with 'err' message if variable is unset or empty
```

Or  the `?` syntax for unset variables:

```yaml
option: "${ENV_VAR?err}" # Vector exits with 'err' message only if variable is unset.
```

## Escaping

You can escape environment variables by prefacing them with a `$` character. For
example `$${HOSTNAME}` or `$$HOSTNAME` is treated literally in the above
environment variable example.

## Interpolation Pitfalls


It is possible to inject multiline blocks.

{{< warning >}}
Support for block interpolation feature may be removed in the future.
We do not recommend using this feature, as it can lead to unwanted injections.
For example, a malicious actor could inject components that execute arbitrary code.
Of course this assumes said actor penetrated the system and has write access to the environment variables.
{{< /warning >}}

For example, the following section of a config:

```yaml
${SOURCES_BLOCK}
```

will be replaced by the following value:

```shell
export SOURCES_BLOCK="sources:\"
  demo:
    type: demo_logs
    format: json
    interval: 1
```

A better approach is to introduce a config pre-processing step with a mature tool. This also has the benefit of giving the user
more control over the configuration and the result can be inspected before it is passed to Vector. See the following example:

```shell
envsubst < snippet_in.yaml > snippet_out.yaml
cat snippet_out.yaml
sources:"
  demo:
    type: demo_logs
    format: json
    interval: 1
```
