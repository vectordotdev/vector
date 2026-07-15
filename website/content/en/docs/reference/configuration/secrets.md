---
title: Secrets configuration reference
short: Secrets
weight: 8
show_toc: true
---

Secrets management lets you keep sensitive configuration values like API keys, passwords, and tokens out of your Vector configuration files. Instead of writing a secret's plaintext value directly into a config option, you configure a secret backend and reference the secret with `SECRET[<backend name>.<secret name>]`. Vector resolves these references by querying the backend when it loads the configuration, before any other config processing happens.

This is the recommended way to supply secrets to Vector, in preference to [environment variable interpolation](/docs/reference/environment_variables/), which can potentially leak variables, lead to template injection, and other security issues. Secret backends never write the resolved value to the environment and are not susceptible to such issues.

## Usage

Configure one or more backends under the top-level `secret` option, then reference secrets from that backend anywhere in your configuration using the `SECRET[<backend name>.<secret name>]` syntax:

```yaml
secret:
  backend_1:
    type: "exec"
    command: ["/path/to/cmd1"]

sources:
  my_source_id:
    type: "aws_sqs"
    region: "us-east-1"
    queue_url: "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
    auth:
      access_key_id: "SECRET[backend_1.aws_access_key_id]"
      secret_access_key: "SECRET[backend_1.aws_secret_access_key]"
```

Here, `auth.access_key_id` and `auth.secret_access_key` are resolved using secrets named `aws_access_key_id` and `aws_secret_access_key`, retrieved from the `backend_1` secret backend. You can reference the same backend from multiple places in your configuration, and you can configure multiple backends if you need to pull secrets from more than one source.

The secret name portion supports `.`, `-`, and `/` characters, so backends that key secrets hierarchically (such as the `directory` backend, below) can be referenced like `SECRET[backend_1.nested/secret_name]`.

If a `SECRET[...]` reference can't be resolved, for example because the backend doesn't recognize the requested name, returns an error, or returns an empty value, Vector logs the error and exits during configuration loading. Secrets are never partially applied.

## Backends

Vector ships with the following secret backend types.

### `exec`

The `exec` backend runs an external command and exchanges secrets with it as JSON over stdin/stdout. This is the most flexible option: it can be used to integrate with any secret store (such as [Vault](https://www.vaultproject.io/)) by wrapping the store's CLI or API in a small script.

```yaml
secret:
  backend_1:
    type: "exec"
    command: ["/path/to/cmd1"]
```

When Vector starts, it calls the configured command with the requested secret names provided as JSON on stdin:

```json
{"version": "1.0", "secrets": ["aws_access_key_id", "aws_secret_access_key"]}
```

The command is expected to write the secrets to stdout as JSON, in this format:

```json
{
  "aws_access_key_id": {"value": "AKIAIOSFODNN7EXAMPLE", "error": null},
  "aws_secret_access_key": {"value": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY", "error": null}
}
```

If a secret can't be provided, return `"error"` set to a message instead of `"value"`. If the command exits with a non-zero status, Vector treats the whole request as failed.

By default, the command must self-discover which backend/config to use for each request (protocol `v1`, shown above). If you want Vector to pass backend-specific configuration to a single generic executable instead (for example, to have one script serve multiple `exec` backend instances), use protocol `v1_1` and set `backend_type`/`backend_config`:

```yaml
secret:
  backend_1:
    type: "exec"
    command: ["/path/to/cmd1"]
    protocol:
      version: "v1_1"
      backend_type: "vault"
      backend_config:
        address: "https://vault.example.com"
```

This sends an additional `type` and `config` field on the request:

```json
{"version": "1.1", "secrets": ["aws_access_key_id"], "type": "vault", "config": {"address": "https://vault.example.com"}}
```

### `file`

The `file` backend reads secrets from a single JSON file on disk, where each secret name maps to its plaintext value:

```yaml
secret:
  backend_1:
    type: "file"
    path: "/path/to/secret"
```

```json
{
  "aws_access_key_id": "AKIAIOSFODNN7EXAMPLE",
  "aws_secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
}
```

This is convenient when a secret manager can materialize its output as a single file, for example a Kubernetes Secret mounted as a volume.

### `directory`

The `directory` backend reads each secret from its own file within a directory, where the file name is the secret name and the file contents are the secret value. This matches the layout Kubernetes uses when a Secret with multiple keys is mounted as a volume, and the layout Docker/Podman secrets and `tmpfs`-backed credential directories typically use.

```yaml
secret:
  backend_1:
    type: "directory"
    path: "/path/to/secrets"
    remove_trailing_whitespace: true
```

```shell
/path/to/secrets/aws_access_key_id
/path/to/secrets/aws_secret_access_key
```

Set `remove_trailing_whitespace: true` if the files may contain a trailing newline (common when they're generated with a shell command), otherwise that newline becomes part of the secret value.

Secret names containing `/` are resolved relative to `path`, so `SECRET[backend_1.nested/aws_access_key_id]` reads `/path/to/secrets/nested/aws_access_key_id`.

### `aws_secrets_manager`

The `aws_secrets_manager` backend retrieves a single secret from [AWS Secrets Manager](https://aws.amazon.com/secrets-manager/) by its secret ID. The secret's value in Secrets Manager must itself be a JSON object mapping secret names to values, the same shape used by the `file` backend, since one Secrets Manager secret can back multiple `SECRET[...]` references:

```yaml
secret:
  backend_1:
    type: "aws_secrets_manager"
    secret_id: "my-secret-id"
    region: "us-east-1"
```

```json
{
  "aws_access_key_id": "AKIAIOSFODNN7EXAMPLE",
  "aws_secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
}
```

This backend supports the same `auth`, `region`/`endpoint`, and `tls` options as Vector's other AWS components, such as the [`aws_s3` source](/docs/reference/configuration/sources/aws_s3/).

{{< config-cross-links group="secrets" >}}

{{< config/group group="secrets" >}}
