---
description: A guide on debugging and troubleshooting Vector
---

# Troubleshooting Guide

This guide covers troubleshooting Vector. The sections are intended to be
followed in order.

First, we're sorry to hear that you are having trouble with Vector. Reliability
and operator friendliness are _very_ important to us, and we urge you to
[open an issue][url.new_bug_report] to let us know what's going on. This helps
us improve Vector.

## 1. Check Vector's logs

We've taken great care to ensure Vector's logs are high-quality and helpful.
In most cases the logs will surface the issue:

{% tabs %}
{% tab title="Manual" %}
If you are not using a service manager, and you're redirecting Vector's
output to a file then you can use a utility like `tail` to access your logs:

```bash
tail /var/log/vector.log
```
{% endtab %}
{% tab title="Systemd" %}
Tail logs:

```bash
sudo journalctl -fu vector
```
{% endtab %}
{% tab title="Initd" %}
Tail logs:

```bash
tail -f /var/log/vector.log
```
{% endtab %}
{% tab title="Homebrew" %}
Tail logs:

```bash
tail -f /usr/local/var/log/vector.log
```
{% endtab %}
{% endtabs %}

## 2. Enable backtraces

{% hint style="info" %}
You can skip to the [next section](#3-enable-debug-logging) if you do not
see an exception in your vector logs.
{% endhint %}

If you see an exception in Vector's logs then we've clearly found the issue.
Before you report a bug, please enable backtraces:

```bash
RUST_BACKTRACE=full vector --config=/etc/vector/vector.toml
```

Backtraces are _critical_ for debugging errors. Once you have the backtrace
please [open a bug report issue][url.new_bug_report].

## 3. Enable debug logging

If you do not see an error in your Vector logs, and the Vector logs appear
to be frozen, then you'll want to drop your log level to `debug`:

{% hint style="info" %}
Vector [rate limits][docs.monitoring.rate-limiting] logs in the hot path.
As a result, dropping to the `debug` level is safe for production environments.
{% endhint %}

{% tabs %}
{% tab title="Env Var" %}
```bash
LOG=debug vector --config=/etc/vector/vector.toml
```
{% endtab %}
{% tab title="Flag" %}
```bash
vector --verbose --config=/etc/vector/vector.toml
```
{% endtab %}
{% endtabs %}

## 4. Get help

At this point we recommend reaching out to the community for help.

1. If encountered a bug, please [file a bug report][url.new_bug_report]
2. If encountered a missing feature, please [file a feature request][url.new_feature_request].
3. If you need help, [join our chat community][url.vector_chat]. You can post a question and search previous questions.


[docs.monitoring.rate-limiting]: ../../usage/administration/monitoring.md#rate-limiting
[url.new_bug_report]: https://github.com/timberio/vector/issues/new?labels=Type%3A+Bug
[url.new_feature_request]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.vector_chat]: https://chat.vector.dev
