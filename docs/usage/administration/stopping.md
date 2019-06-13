---
description: Stopping Vector
---

# Stopping

This document covers how to properly stop Vector.

## Quick Start

The Vector process should be stopped by sending it a `SIGTERM` process signal:

```bash
kill -SIGTERM <vector-process-id>
```

If you are currently running the Vector process in your terminal, this can be achieved by a single `ctrl+c` key combination.

## How It Works

### Graceful Shutdown

Vector is designed to gracefully shutdown within 20 seconds when a `SIGTERM` process signal is received. The shutdown process is as follows:

1. Stop accepting new data for all [sources](../configuration/sources/).
2. Gracefully close any open connections with a 20 second timeout.
3. Flush any [sink buffers](../configuration/sinks/buffer.md) with a 20 second timeout.
4. Exit the process with a 1 code.

### Force Killing

If Vector is forcefully killed there is potential for losing any in-flight data. To mitigate this we recommend enabling [on-disk buffers](../configuration/sinks/buffer.md#on-disk) and avoiding forceful shutdowns whenever possible.

