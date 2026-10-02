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

Vector parses the configuration file first, then interpolates environment variables
in string values with the following syntax. Keys and comments are not interpolated.

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

Substitution cannot add configuration keys or array elements, but it can still change
the meaning of a string value, including a file path or code in an embedded language.

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

Vector parses configuration before interpolation. Quotes, newlines, braces, and other
configuration syntax in an environment variable remain part of the string value; they
cannot add configuration keys or components. Vector does not escape values for embedded
languages such as VRL, or restrict the files and destinations a substituted value can select.
Operators remain responsible for controlling the content of interpolated environment variables.

Configuration must be valid before interpolation. In TOML and JSON, quote placeholders
even in numeric and boolean fields; Vector converts the resulting string to the field's
declared type. YAML string placeholders also work in these fields:

```yaml
sources:
  demo:
    type: demo_logs
    format: json
    count: "${MY_COUNT}"
```

Use literal map keys and component names. A placeholder cannot expand into multiple
array elements; use a separate string placeholder for each element instead.

Multiline values can be interpolated into a string field. If you need to inject configuration
blocks instead, use a config pre-processing step with a tool like `envsubst`.
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
