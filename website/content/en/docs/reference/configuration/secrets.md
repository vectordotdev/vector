---
title: Secrets configuration reference
short: Secrets
weight: 8
show_toc: true
---

{{< config-cross-links group="secrets" >}}

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

{{< config/group group="secrets" >}}
