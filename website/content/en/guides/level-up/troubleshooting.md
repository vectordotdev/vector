---
title: Troubleshooting
description: A guide to debugging and troubleshooting Vector
author_github: binarylogic
domain: operations
weight: 4
tags: ["troubleshooting", "level up", "guides", "guide"]
---

This guide covers troubleshooting Vector. The sections are intended to be
followed in order. If you'd like to troubleshoot by inspecting events flowing
through your pipeline, please check out the [Vector tap] guide.

First, we're sorry to hear that you're having trouble with Vector! Reliability
and operator friendliness are _very_ important to us, and we urge you to
[open an issue][urls.new_bug_report] to let us know what's going on. This helps
us improve Vector.

## 1. Check for any known issues

Start by searching [Vector's issues][urls.vector_issues]. You can filter
to the specific component via the `label` filter.

## 2. Check Vector's logs

We've taken great care to ensure that Vector's logs are high quality and helpful.
In most cases the logs will surface the issue:

{{< tabs default="Manual" >}}
{{< tab title="Manual" >}}
If you aren't using a service manager and you're redirecting Vector's output to a file, you can use
a utility like `tail` to access your logs:

```shell
tail /var/log/vector.log
```

{{< /tab >}}

{{< tab title="Systemd" >}}
Tail logs:

```shell
sudo journalctl -fu vector
```

{{< /tab >}}

{{< tab title="Initd" >}}
Tail logs:

```shell
tail -f /var/log/vector.log
```

{{< /tab >}}
{{< tab title="Homebrew" >}}
Tail logs:

```shell
tail -f /usr/local/var/log/vector.log
```

{{< /tab >}}
{{< /tabs >}}

## 3. Enable backtraces

{{< info >}}
You can skip to the [next section](#4-enable-debug-logging) if you don't
have an exception in your logs.
{{< /info >}}

If you see an exception in Vector's logs then we've clearly found the issue.
Before you report a bug, please enable backtraces:

```bash
RUST_BACKTRACE=full vector --config=/etc/vector/vector.yaml
```

Backtraces are _critical_ for debugging errors. Once you have the backtrace
please [open a bug report issue][urls.new_bug_report].

## 4. Enable debug logging

If you don't see an error in your Vector logs and the Vector logs appear
to be frozen, then you'll want to drop your log level to `debug`:

{{< info >}}
Vector rate limits logs in the hot path. As a result, dropping to the
`debug` level is safe for production environments.
{{< /info >}}

{{< tabs default="Env Var" >}}
{{< tab title="Env Var" >}}

```shell
VECTOR_LOG=debug vector --config=/etc/vector/vector.yaml
```

{{< /tab >}}
{{< tab title="Flag" >}}

```bash
vector --verbose --config=/etc/vector/vector.yaml
```

{{< /tab >}}
{{< /tabs >}}

## 5. Get help

At this point, we recommend reaching out to the community for help.

1. If you've encountered a bug, please [file a bug report][urls.new_bug_report]

2. If you've identified a missing feature, please [file a feature request][urls.new_feature_request].

3. If you need help, [join our chat community][urls.vector_chat]. You can post a question and search previous questions.

[urls.new_bug_report]: https://github.com/vectordotdev/vector/issues/new?assignees=&labels=type%3A+bug&template=bug.yml
[urls.new_feature_request]: https://github.com/vectordotdev/vector/issues/new?assignees=&labels=type%3A+feature&template=feature.yml
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_issues]: https://github.com/vectordotdev/vector/issues
[Vector tap]: /guides/level-up/vector-tap-guide
