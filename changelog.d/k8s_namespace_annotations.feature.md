The `kubernetes_logs` source now enriches events with the owning Namespace's annotations, in addition to its labels. Add `namespace_annotation_fields.namespace_annotations` to customize or suppress the output field (default: `.kubernetes.namespace_annotations`).

authors: srstrickland
