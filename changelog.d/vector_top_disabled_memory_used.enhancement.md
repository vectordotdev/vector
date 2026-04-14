`vector top` terminal UI now shows `disabled` in the Memory Used column when the connected Vector instance was not started with `--allocation-tracing`, instead of displaying misleading zeros. A new `GetAllocationTracingStatus` gRPC endpoint is queried on connect to determine the status.

authors: pront
