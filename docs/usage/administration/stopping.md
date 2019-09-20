---
description: Stopping Vector
---

# Stopping

The Vector process should be stopped by sending it a `SIGTERM` process signal:

{% tabs %}
{% tab title="Manual" %}
```bash
kill -SIGTERM <vector-process-id>
```
{% endtab %}
{% tab title="Systemd" %}
```bash
sudo systemctl stop vector
```
{% endtab %}
{% tab title="Initd" %}
```bash
/etc/init.d/vector stop
```
{% endtab %}
{% tab title="Homebrew" %}
```bash
brew services stop vector
```
{% endtab %}
{% endtabs %}

If you are currently running the Vector process in your terminal, this can be
achieved by a single `ctrl+c` key combination.

## Graceful Shutdown

Vector is designed to gracefully shutdown within 20 seconds when a `SIGTERM`
process signal is received. The shutdown process is as follows:

1. Stop accepting new data for all [sources][docs.sources].
2. Gracefully close any open connections with a 20 second timeout.
3. Flush any sink buffers with a 20 second timeout.
4. Exit the process with a 1 code.

## Force Killing

If Vector is forcefully killed there is potential for losing any in-flight
data. To mitigate this we recommend enabling on-disk buffers and avoiding
forceful shutdowns whenever possible.


[docs.sources]: ../../usage/configuration/sources/README.md
