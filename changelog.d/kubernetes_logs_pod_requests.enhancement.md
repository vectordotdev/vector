The `kubernetes_logs` source now supports adding effective Pod CPU and memory
requests through `pod_annotation_fields.pod_cpu_request_millicores` and
`pod_annotation_fields.pod_memory_request_bytes`. The totals account for
application containers, restartable init containers, peak initialization
requests, explicit Pod-level requests, and Pod overhead. These fields are opt-in
in the legacy log namespace and available by default in Vector metadata.

authors: jonahgetz
