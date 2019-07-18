---
description: Vector environment variables
---

# Environment Variables

Vector recognizes the following environment variables:

| Name                        | Description                                                                      |
|:----------------------------|:---------------------------------------------------------------------------------|
| `AWS_ACCESS_KEY_ID=xxx`     | Used for AWS authentication. See relevant AWS [sinks][docs.sinks] for more info. |
| `AWS_SECRET_ACCESS_KEY=xxx` | Used for AWS authentication. See relevant AWS [sinks][docs.sinks] for more info. |
| `LOG="info"`                | Sets Vector's [log level][docs.monitoring.logs].                                 |
| `RUST_BACKTRACE=full`       | Enables backtraces for logging errors.                                           |

More options can be set with [flags][docs.starting.flags] when
[starting][docs.starting] Vector.


[docs.monitoring.logs]: ../../usage/administration/monitoring.md#logs
[docs.sinks]: ../../usage/configuration/sinks
[docs.starting.flags]: ../../usage/administration/starting.md#flags
[docs.starting]: ../../usage/administration/starting.md
