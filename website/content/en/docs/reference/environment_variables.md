---
title: Environment Variables
short: Environment Variables
weight: 5
tags: ["env", "environment variables", "interpolation"]
---

By default, environment variable interpolation is disabled (the default changed in v0.57.0). To enable it, pass
`--dangerously-allow-env-var-interpolation` to the `vector` CLI, or set the environment variable
`VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`.

{{< warning >}}
Environment variables can be read by any user that is able to read `/proc/<PID>/environ` (or similar
in other operating systems) of
the running Vector process, regardless of whether interpolation is enabled. Operators are
advised not to include sensitive data in environment variables and are encouraged to use the
[secrets backend](/docs/reference/configuration/secrets/) instead.
{{< /warning >}}

## Usage

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

## How interpolation can be misused

Vector configuration templates can use environment variable interpolation, for example:

```yaml
sources:
  app_logs:
    type: file
    include:
      - "${LOG_PATH}"
```

If an attacker can influence the value of LOG_PATH, they can point Vector at any file the process can read, including sensitive system files:

```shell
export LOG_PATH=/etc/shadow
```

After substitution, Vector reads `/etc/shadow` as if it were a log file and forward its contents to whatever sink is configured, leaking password hashes or other sensitive data.

This is one example of the risks that environment variable interpolation exposes. Environment variable interpolation is disabled by default for this reason.

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

Environment variable interpolation is disabled by default. Only enable it
with `--dangerously-allow-env-var-interpolation` if you fully control every environment variable accessible
to the Vector process and accept that environment variables may leak to users that have access to
the Vector process.

Even when enabled, Vector prevents some security issues related to environment variable interpolation by rejecting environment variables that contain newline
characters. This also prevents injection of multi-line configuration blocks.

Vector does not validate or escape other characters in interpolated values. Values containing config-structural characters such as
`"`, `{`, `}`, `[`, or `]` are substituted verbatim before the config file is parsed, and may affect the resulting parsed structure.
Operators are responsible for controlling the content of interpolated environment variables.

If you need to inject multi-line configuration blocks, use a config pre-processing step with a tool like `envsubst`.
This approach gives you more control over the configuration and allows you to inspect the result before passing it to Vector.
Note that `envsubst` only expands plain `$VAR` and `${VAR}` references; it does not understand Vector's extended syntax
such as `${VAR:-default}` or `${VAR:?err}`, which are passed through unchanged.

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
