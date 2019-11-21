---
title: Environment Variables
description: Vector environment variables
---

Vector recognizes the following environment variables:

| Name                        | Description                                                                      |
|:----------------------------|:---------------------------------------------------------------------------------|
| `AWS_ACCESS_KEY_ID=xxx`     | Used for AWS authentication. See relevant AWS [sinks][docs.sinks] for more info. |
| `AWS_SECRET_ACCESS_KEY=xxx` | Used for AWS authentication. See relevant AWS [sinks][docs.sinks] for more info. |
| `LOG="info"`                | Sets Vector's [log level][docs.monitoring#logs].                                 |
| `RUST_BACKTRACE=full`       | Enables backtraces for logging errors.                                           |

More options can be set with [flags][docs.process-management#flags] when
[starting][docs.process-management#starting] Vector.


[docs.monitoring#logs]: /docs/administration/monitoring#logs
[docs.process-management#flags]: /docs/administration/process-management#flags
[docs.process-management#starting]: /docs/administration/process-management#starting
[docs.sinks]: /docs/reference/sinks
