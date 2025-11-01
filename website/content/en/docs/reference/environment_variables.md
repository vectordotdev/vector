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

## Security Restrictions

Vector prevents security issues related to environment variable interpolation by rejecting environment variables that contain newline
characters. This also prevents injection of multi-line configuration blocks.

If you need to inject multi-line configuration blocks, use a config pre-processing step with a tool like `envsubst`.
This approach gives you more control over the configuration and allows you to inspect the result before passing it to Vector:

```shell
# config_template.yaml
${SOURCES_BLOCK}
sinks:
  console:
    type: console
    inputs: ["demo"]
    encoding:
      codec: json
```

```shell
# Export multi-line block
export SOURCES_BLOCK="sources:
  demo:
    type: demo_logs
    format: json
    interval: 1"

# Process template and inspect result
envsubst < config_template.yaml > config.yaml

# Start Vector with processed config
vector --config config.yaml
```
