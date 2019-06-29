---
description: Reloading the Vector process to recognize configuration changes
---

# Reloading

Vector can be reloaded, on the fly, to recognize any configuration changes by
sending the Vector process a `SIGHUP` signal:

{% tabs %}
{% tab title="Manual" %}
```bash
kill -SIGHUP <vector-process-id>
```

You can find the Vector process ID with:

```bash
ps -ax vector | grep vector
```
{% endtab %}
{% tab title="Systemd" %}
```bash
systemctl kill -s HUP --kill-who=main vector.service
```
{% endtab %}
{% tab title="Initd" %}
```bash
/etc/init.d/vector reload
```
{% endtab %}
{% tab title="Homebrew" %}
```bash
kill -SIGHUP <vector-process-id>
```

You can find the Vector process ID with:

```bash
ps -ax vector | grep vector
```
{% endtab %}
{% endtabs %}

## Configuration Errors

When Vector is reloaded it proceeds to read the new configuration file from
disk. If the file has errors it will be logged to `STDOUT` and ignored,
preserving any previous configuration that was set. If the process exits you
will not be able to restart the process since it will proceed to use the
new configuration file. It is _highly_ recommended that you
[validate your configuration](validating-configuration.md) before deploying
it to a running instance of Vector.

## Graceful Pipeline Transitioning

Vector will perform a diff between the new and old configuration, determining
which sinks and sources should be started and shutdown and ensures the
transition from the old to new pipeline is graceful.



