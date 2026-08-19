Fixed an issue where unusually deeply nested event data or metadata could make disk buffers unreadable or cause vector-to-vector pipelines to retry indefinitely. Vector now detects affected events before buffering or sending while leaving safely nested events unchanged. When when_full = "overflow" is configured, the original event is routed intact to the overflow stage regardless of buffer occupancy; otherwise, only the affected event is dropped.

authors: connoryy ganelo EricaJ6 jonodera97
