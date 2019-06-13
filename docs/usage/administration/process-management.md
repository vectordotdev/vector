---
description: Managing the Vector process
---

# Process Management

This page will cover Vector's process behavior, such as [output](process-management.md#output), [signals](process-management.md#signals), and [exit codes](process-management.md#exit-codes).

## Output

Vector writes all output to `STDOUT`, including errors. Vector currently does not use `STDERR`.

## Signals

Vector handles the following [process signals](https://bash.cyberciti.biz/guide/Sending_signal_to_Processes).

| Signal | Code | Behavior |
| :--- | :--- | :--- |
| `SIGHUP` | `1` | Reloads configuration from the originally specified configuration file. |
| `SIGTERM` | `15` | Gracefully shuts down Vector. |

## Exit Codes

When Vector exits, it will exit with the following codes:

| Code | Description |
| :--- | :--- |
| `1` | Vector exited normally without issue. |
| `65` | Input data was incorrect. This usually means there was an error in your [configuration file](../configuration/), or the flags passed were incorrect. Vector will print a descriptive error message to `STDERR`. |

