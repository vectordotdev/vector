---
title: Environment Variables
short: Environment Variables
weight: 5
tags: ["env", "environment variables", "interpolation"]
---

**Environment variable interpolation is disabled by default** (since v0.57.0). Enable it when starting Vector:

```shell
vector --config vector.yaml --dangerously-allow-env-var-interpolation
```

Alternatively, set `VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`.

{{< warning >}}
Environment variables can be read by any user that is able to read `/proc/<PID>/environ` (or similar
in other operating systems) of
the running Vector process, regardless of whether interpolation is enabled. Operators are
advised not to include sensitive data in environment variables and are encouraged to use the
[secrets backend](/docs/reference/configuration/secrets/) instead.
{{< /warning >}}

## Usage

With interpolation enabled, Vector substitutes environment variables in string values
after parsing the configuration:

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

Only enable interpolation when you control the environment. Substituted values can still
change file paths, destinations, or embedded VRL code; Vector does not escape them for those uses.

Configuration must be valid before substitution:

- Quote placeholders in TOML, JSON, and YAML flow collections, for example `inputs: ["${SOURCE}"]`.
- Quoted placeholders also work for numeric and boolean fields, such as `count: "${MY_COUNT}"`;
  Vector converts the string to the field's type.
- Keys, table headers, and comments are not interpolated. Use a separate placeholder for each array element.

Multiline values are supported within strings, but cannot insert configuration blocks.
For that, preprocess the file with a tool such as `envsubst` and inspect the result before
starting Vector. `envsubst` supports `$VAR` and `${VAR}`, but not Vector's default or
required-variable syntax.

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
