---
description: Vector environment variables
---

# Environment Variables

Vector recognizes the following environment variables:

| Name | Description |
| :--- | :---------- |
| `LOG="info"` | Sets Vector's [log level][docs.monitoring.logs]. |
| `RUST_BACKTRACE=full` | Enables backtraces for logging errors. |

More oprtions can be set with [flags][docs.starting.flags] when
[starting][docs.starting] Vector.


[docs.monitoring.logs]: ../../usage/administration/monitoring.md#logs
[docs.starting.flags]: ../../usage/administration/starting.md#flags
[docs.starting]: ../../usage/administration/starting.md
