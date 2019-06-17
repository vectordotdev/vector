---
description: Reloading the Vector process to recognize configuration changes
---

# Reloading

This document covers how to reload Vector's configuration without stopping the process.

## Quick Start

Vector can be reloaded, on the fly, to recognize any configuration changes by sending the Vector process a `SIGHUP` signal.

```bash
kill -SIGHUP <vector-process-id>
```

## How It Works

### Configuration Errors

When Vector is reloaded it proceeds to read the new configuration file from disk. If the file has errors it will be logged to `STDOUT` and ignored, preserving any previous configuration that was set. If the process exits you will not be able to restart the process since it will proceed to use the new configuration file. It is _highly_ recommended that you [validate your configuration](validating-configuration.md) before deploying it to a running instance of Vector.

### Graceful Pipeline Transitioning

Vector will perform a diff between the new and old configuration, determining which sinks and sources should be started and shutdown. The process is as follows:

1. Old sources stop accepting data.
2. Old source connections are shutdown with a 20 second timeout.
3. Old sinks are flushed.
4. Once the old sources have been successfully shutdown, new sources are started. The delay is intentional in order to ensure we do not exceed system resources.
5. New sinks are started.



