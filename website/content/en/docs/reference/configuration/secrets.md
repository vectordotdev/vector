---
title: Secrets configuration reference
short: Secrets
weight: 8
show_toc: true
---

{{< config-cross-links group="secrets" >}}

Secrets management lets you keep sensitive configuration values like API keys, passwords, and tokens out of your Vector configuration files. Instead of writing a secret's plaintext value directly into a config option, you configure a secret backend and reference the secret with `SECRET[<backend name>.<secret name>]`. Vector resolves these references by querying the backend when it loads the configuration, before any other config processing happens.

This is the recommended way to supply secrets to Vector, in preference to [environment variable interpolation](/docs/reference/environment_variables/). Unlike environment variables, secret values are never written to the Vector process's environment, so they can't leak to anyone with access to `/proc/<PID>/environ` or similar. Note that, like environment variable interpolation, secret resolution substitutes the retrieved value directly into the configuration text before it's parsed, so a secret value containing YAML/TOML syntax can still alter the parsed configuration; only pull secrets from backends and paths you trust.

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

The backend name portion supports only letters, digits, and `_`; backend names containing `-` aren't addressable from a `SECRET[...]` reference ([known issue](https://github.com/vectordotdev/vector/issues/25849)), even though `-` is otherwise a valid character in a backend's component ID. The secret name portion is more permissive and supports `.`, `-`, and `/` characters, so backends that key secrets hierarchically (such as the `directory` backend, below) can be referenced like `SECRET[backend_1.nested/secret_name]`.

Text that matches the `SECRET[<backend name>.<secret name>]` grammar but can't be resolved — for example, because the backend doesn't recognize the requested secret name, returns an error, or returns an empty value — causes Vector to log the error and exit during configuration loading. Secrets are never partially applied. Text that doesn't match the grammar at all, for example a backend name containing `-`, is left in the configuration as a literal string instead, with no resolution error.

{{< config/group group="secrets" >}}
