The `kubernetes_logs` source no longer drops Namespace and Node labels from the Vector log namespace metadata (`%kubernetes_logs.namespace_labels` and `%kubernetes_logs.node_labels`) when `namespace_annotation_fields.namespace_labels` or `node_annotation_fields.node_labels` is set to `""`. An empty path now only suppresses the legacy event field, matching how `pod_annotation_fields` behaves.

authors: srstrickland
