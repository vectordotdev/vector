With log namespacing, `kubernetes_logs` includes CPU and memory requests for the logging
container and the whole Pod in metadata. To add these fields to legacy logs, set their
paths in `pod_annotation_fields`.

authors: jonahgetz
