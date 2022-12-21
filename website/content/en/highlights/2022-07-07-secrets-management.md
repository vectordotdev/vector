---
date: "2022-07-07"
title: "Load secrets into Vector"
description: "A new mechanism to load secrets into Vector from an external process"
authors: ["jszwedko"]
pr_numbers: [11985]
release: "0.23.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

With this release, we have introduced a new mechanism to load secrets securely into Vector by calling an external
process. This can be used, for example, to integrate with a service like [Vault](https://www.vaultproject.io/) to
provide credentials.

Previously the preferred mechanism was injection via environment variables, but there are some security concerns as
these values can be read if a user on a host has access to read from the `/proc` filesystem for the Vector process.

A secret backend can be configured like this:

```toml
[secret.backend_1]
type = "exec" # exec is the only supported backend as of writing
command = ["/path/to/cmd1"]
```

You can then specify where secrets should be read via `SECRET[<backend name>.<secret name>]` in the config like:

```toml
[sources.my_source_id]
type = "aws_sqs"
region = "us-east-1"
queue_url = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
auth.access_key_id = "SECRET[backend_1.aws_access_key_id]"
auth.secret_access_key = "SECRET[backend_1.aws_secret_access_key]"
```

Here `auth.access_key_id` and `auth.secret_access_key` will use secrets provided by the `backend_1` secret backend.

When Vector starts, it will call the configured secret backend command, here `/path/to/cmd1`, with the needed secrets
provided as JSON on stdin:

```json
{"version": "1.0", "secrets": ["aws_access_key_id", "aws_secret_access_key"]}
```


The command is then expected to write the secrets to stdout as JSON in the following format:

```json
{
  "aws_access_key_id": {"value": "AKIAIOSFODNN7EXAMPLE", "error": null},
  "aws_secret_access_key": {"value": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY", "error": null}
}
```

Vector will then use the returned values when loading the configuration.

If an `error` is returned, or the command exits non-zero, Vector will log any errors and stop.

See the [documentation](/docs/reference/configuration/global-options/#secret) for additional details.
