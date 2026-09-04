---
title: Secrets configuration reference
short: Secrets
weight: 8
show_toc: true
---

{{< config-cross-links group="secrets" >}}

Secrets management lets you keep sensitive configuration values like API keys, passwords, and tokens out of your Vector configuration files.

This is the recommended way to supply secrets to Vector, in preference to [environment variable interpolation](/docs/reference/environment_variables/). Unlike environment variables, secret values are never written to the Vector process's environment, so they can't leak to anyone with access to `/proc/<PID>/environ` or similar.

## Usage

Configure one or more backends under the top-level `secret` option, then reference secrets from those backends anywhere in your configuration using the `SECRET[<backend name>.<secret name>]` syntax. Vector collects the referenced secret names and queries each required backend when it loads the configuration, including during configuration reloads. It substitutes the retrieved values before parsing the configuration. A secret value containing YAML or TOML syntax can therefore alter the parsed configuration, so only retrieve secrets from backends and paths you trust.

You can reference the same backend from multiple places in your configuration, and you can configure multiple backends if you need to retrieve secrets from more than one source. Backend names support letters, digits, `_`, and `-`. Secret names additionally support `.` and `/`, allowing hierarchical keys such as `SECRET[backend_1.database/password]`.

If a referenced backend or secret doesn't exist, a backend reports an error, or a backend returns an empty value, configuration loading fails. Secrets are never partially applied.

### Exec backend

The `exec` backend runs a command that retrieves the referenced secrets. The first item in `command` is the executable, and any remaining items are passed to it as arguments:

```yaml
secret:
  external:
    type: "exec"
    command: ["/usr/local/bin/read-secrets", "--environment", "production"]
    timeout: 5

sinks:
  datadog:
    type: "datadog_logs"
    inputs: ["logs"]
    default_api_key: "SECRET[external.datadog_api_key]"
```

#### Protocol v1

Vector writes a single JSON request to the command's standard input. With the default `v1` protocol, the request contains the protocol version and all referenced secret names:

```json
{
  "version": "1.0",
  "secrets": ["datadog_api_key"]
}
```

The order of `secrets` is unspecified. The command must write a single JSON object to standard output, with one entry for every requested secret:

```json
{
  "datadog_api_key": {
    "value": "example-api-key",
    "error": null
  }
}
```

For a secret that couldn't be retrieved, return an error instead of a value:

```json
{
  "datadog_api_key": {
    "value": null,
    "error": "secret not found"
  }
}
```

Errors, missing entries, empty values, and malformed output cause configuration loading to fail. Standard output must contain only the response JSON. The command can write diagnostic messages to standard error, which Vector logs as warnings.

#### Protocol v1_1

Protocol `v1_1` additionally passes a backend type and its configuration to the command. This supports executables that provide multiple secret storage implementations:

```yaml
secret:
  external:
    type: "exec"
    command: ["/usr/local/bin/read-secrets"]
    protocol:
      version: "v1_1"
      backend_type: "file.json"
      backend_config:
        file_path: "/etc/vector/secrets.json"
```

The corresponding request uses protocol version `1.1` and includes the configured `type` and `config` values. Its response format is the same as for `v1`:

```json
{
  "version": "1.1",
  "secrets": ["datadog_api_key"],
  "type": "file.json",
  "config": {
    "file_path": "/etc/vector/secrets.json"
  }
}
```

### File backend

The `file` backend retrieves secrets from a UTF-8 JSON file:

```yaml
secret:
  local_file:
    type: "file"
    path: "/etc/vector/secrets.json"
```

The file must contain a JSON object that maps secret names directly to string values:

```json
{
  "username": "vector",
  "password": "example-password"
}
```

For example, `SECRET[local_file.username]` resolves to `vector`. Unlike the `exec` response format, file values aren't wrapped in `value` and `error` fields. Every referenced secret must exist and have a non-empty string value.

### Directory backend

The `directory` backend retrieves each secret from a separate UTF-8 file under the configured path:

```yaml
secret:
  local_directory:
    type: "directory"
    path: "/etc/vector/secrets"
    remove_trailing_whitespace: true
```

The relative file path is the secret name, so secrets can be organized into nested directories:

```text
/etc/vector/secrets/
├── api_key
└── database/
    ├── username
    └── password
```

With this layout, `SECRET[local_directory.api_key]` reads `api_key`, while `SECRET[local_directory.database/username]` reads `database/username`. The complete file contents become the secret value. By default, trailing whitespace, including a final newline, is retained. Set `remove_trailing_whitespace` to `true` to remove it.

### AWS Secrets Manager backend

For configuration examples, authentication options, and operational guidance, see the [AWS Secrets Manager guide](/guides/aws/aws-secrets-manager/) and its [complete example](/guides/aws/aws-secrets-manager-example/).

## Configuration reference

{{< config/group group="secrets" >}}
